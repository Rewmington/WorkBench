/// Anvil 同步信封的 Dart 侧实现 —— 与 PC 的 `src-tauri/src/backup/crypto.rs`
/// 是**同一份格式的两个实现**，不是两个格式。
///
/// 头部布局（逐字节对齐 Rust）：
/// ```text
/// "ANVILBKP"(8) | ver(1) | kdf(1) | m_kib(4,BE) | t_cost(4,BE) | p_cost(1) | salt(16) | nonce(12) | 密文‖tag(16)
/// ```
/// AAD 取「nonce 之前的全部头部字节」：salt 或 kdf 参数被人换掉时认证标签直接不通过，
/// 而不是悄悄用一把错的钥匙解出乱码。这句话在 Rust 侧是同一份实现，两边不能各写各的。
///
/// 参数写在头部里、`open` 会夹住上限，所以**读哪份文件就用哪份文件的成本**。
/// 一次同步 = 自己 seal 一次 + 开对方那份一次 = 两次派生，成本由 [KdfParams.sync] 决定
/// （日备用的 64 MiB 是给「一天一次」定的，不是给「每次打开 app」定的）。
library;

import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:cryptography/cryptography.dart';
import 'package:cryptography/dart.dart';

const _magic = [0x41, 0x4e, 0x56, 0x49, 0x4c, 0x42, 0x4b, 0x50]; // ANVILBKP
const _version = 1;
const _kdfArgon2id = 1;
const _saltLen = 16;
const _nonceLen = 12;
const _headerLen = 8 + 1 + 1 + 4 + 4 + 1 + _saltLen + _nonceLen; // 47
const _aadLen = _headerLen - _nonceLen; // 35
const _tagLen = 16;

/// 头部是别人写的，不可信。上限是「手机愿意为一个还没验签的文件付出的成本」，不是
/// 「历史上见过的最大值」：认证标签要到派生之后才校验，1 GiB 那档足够在拒绝之前把
/// 手机打到 OOM。Anvil 当前只用 16 MiB（同步）和 64 MiB（日备），这里给到 128 MiB
/// 留未来调参的余量 —— 与 Rust 侧 MAX_* 三个常量同一组值。
const _maxMKib = 131072; // 128 MiB
const _maxTCost = 8;
const _maxPCost = 4;

/// 信封不合法时的说明。文字直接进界面那一行状态，所以要说人话。
class EnvelopeException implements Exception {
  final String message;
  EnvelopeException(this.message);
  @override
  String toString() => message;
}

/// Argon2id 的三个成本参数。
class KdfParams {
  final int mKib;
  final int tCost;
  final int pCost;

  const KdfParams({required this.mKib, required this.tCost, required this.pCost});

  /// 日备那把：与 Rust 的 `crypto::M_KIB/T_COST/P_COST` 同一个值。
  const KdfParams.backup() : mKib = 65536, tCost = 3, pCost = 1;

  /// 同步用的候选：手机每次打开 app 都要派生两次，成本按实测挑（PLAN-MOBILE §7 第 1 条）。
  const KdfParams.sync() : mKib = 16384, tCost = 3, pCost = 1;
}

class Envelope {
  Envelope._();

  static final _random = Random.secure();

  /// `secret` 是 `邮箱:应用密码` —— 邮箱也进 KDF，这样同一个应用密码在别的账号下解不开。
  static Future<Uint8List> seal(String secret, Uint8List plaintext,
      {KdfParams params = const KdfParams.sync()}) async {
    final salt = _bytes(_saltLen);
    final nonce = _bytes(_nonceLen);
    final key = await _derive(secret, salt, params);

    final header = Uint8List(_aadLen);
    header.setRange(0, 8, _magic);
    header[8] = _version;
    header[9] = _kdfArgon2id;
    _putU32(header, 10, params.mKib);
    _putU32(header, 14, params.tCost);
    header[18] = params.pCost;
    header.setRange(19, 19 + _saltLen, salt);
    // nonce 紧跟在 AAD 之后，位置是 [_aadLen, _headerLen)
    final cipher = DartAesGcm();
    final box = await cipher.encrypt(plaintext,
        nonce: nonce, secretKey: key, aad: header.sublist(0, _aadLen));
    final ct = box.cipherText;
    final tag = box.mac.bytes;

    final out = Uint8List(_headerLen + ct.length + tag.length);
    out.setRange(0, _aadLen, header);
    out.setRange(_aadLen, _headerLen, nonce);
    out.setRange(_headerLen, _headerLen + ct.length, ct);
    out.setRange(_headerLen + ct.length, out.length, tag);
    return out;
  }

  /// 打开。用的是**文件头部自带的**成本参数，所以两端各写各的参数量级也不会互相解不开。
  static Future<Uint8List> open(String secret, Uint8List blob) async {
    if (blob.length < _headerLen + _tagLen) {
      throw EnvelopeException('文件太短，不像 Anvil 的同步信封');
    }
    for (var i = 0; i < 8; i++) {
      if (blob[i] != _magic[i]) {
        throw EnvelopeException('这不是 Anvil 的信封（魔数不匹配）');
      }
    }
    if (blob[8] != _version) {
      throw EnvelopeException('信封版本不支持');
    }
    if (blob[9] != _kdfArgon2id) {
      throw EnvelopeException('不支持的密钥派生方式');
    }
    final mKib = _u32(blob, 10);
    final tCost = _u32(blob, 14);
    final pCost = blob[18];
    if (mKib == 0 || mKib > _maxMKib || tCost == 0 || tCost > _maxTCost ||
        pCost == 0 || pCost > _maxPCost) {
      throw EnvelopeException(
          '信封头部的 KDF 参数超出可接受范围（m=$mKib KiB / t=$tCost / p=$pCost），拒绝派生');
    }
    final salt = Uint8List.fromList(blob.sublist(19, 19 + _saltLen));
    final nonce =
        Uint8List.fromList(blob.sublist(_aadLen, _headerLen));
    final ct = blob.sublist(_headerLen, blob.length - _tagLen);
    final mac = Mac(Uint8List.fromList(blob.sublist(blob.length - _tagLen)));

    final key = await _derive(
        secret, salt, KdfParams(mKib: mKib, tCost: tCost, pCost: pCost));
    try {
      final plain = await DartAesGcm().decrypt(
        SecretBox(ct, nonce: nonce, mac: mac),
        secretKey: key,
        aad: blob.sublist(0, _aadLen),
      );
      return Uint8List.fromList(plain);
    } catch (_) {
      // 与 Rust 侧同一句话：不说「文件坏了」，因为最常见的原因是密码敲错
      throw EnvelopeException('解密失败：应用密码不对，或这份信封来自另一台机器/被改过');
    }
  }

  static Future<SecretKey> _derive(String secret, List<int> salt, KdfParams p) async {
    final argon2 = DartArgon2id(
      memory: p.mKib,
      iterations: p.tCost,
      parallelism: p.pCost,
      hashLength: 32,
    );
    return argon2.deriveKey(
      secretKey: SecretKey(utf8.encode(secret)),
      nonce: salt,
    );
  }

  static Uint8List _bytes(int n) {
    final b = Uint8List(n);
    for (var i = 0; i < n; i++) {
      b[i] = _random.nextInt(256);
    }
    return b;
  }
}

void _putU32(Uint8List at, int offset, int value) {
  at[offset] = (value >> 24) & 0xff;
  at[offset + 1] = (value >> 16) & 0xff;
  at[offset + 2] = (value >> 8) & 0xff;
  at[offset + 3] = value & 0xff;
}

int _u32(Uint8List at, int offset) =>
    (at[offset] << 24) | (at[offset + 1] << 16) | (at[offset + 2] << 8) | at[offset + 3];
