/// 药箱规则的移植测试：用例与药箱仓库 `test/widget_test.dart` 同一批，
/// 改这里之前先看那边为什么这么断言。
library;

import 'package:anvil_mobile/medicine.dart';
import 'package:flutter_test/flutter_test.dart';

Medicine med({
  String id = 'm1',
  String name = '维生素D',
  double remaining = 21,
  double perBox = 30,
  double doses = 1,
  double amount = 1,
  int lowDays = 7,
  String times = '',
  int updatedAt = 100,
  bool deleted = false,
}) =>
    Medicine(
      id: id,
      name: name,
      perBox: perBox,
      remaining: remaining,
      dosesPerDay: doses,
      amountPerDose: amount,
      unit: '片',
      lowDays: lowDays,
      note: '',
      times: times,
      createdAt: 1,
      updatedAt: updatedAt,
      deleted: deleted,
    );

DoseLog log(String id, String medId, int at, {double amount = 1, int updatedAt = 100, bool deleted = false}) =>
    DoseLog(
      id: id,
      medicineId: medId,
      takenAt: at,
      amount: amount,
      note: '',
      updatedAt: updatedAt,
      deleted: deleted,
    );

Map<String, Object?> medRow(String id, {int updatedAt = 100, double remaining = 10, int deleted = 0, String name = '药'}) => {
      'id': id,
      'name': name,
      'per_box': 30.0,
      'remaining': remaining,
      'doses_per_day': 1.0,
      'amount_per_dose': 1.0,
      'unit': '片',
      'low_days': 7,
      'note': '',
      'times': '08:00',
      'created_at': 1,
      'updated_at': updatedAt,
      'deleted': deleted,
    };

Map<String, Object?> logRow(String id, String medId, int at, {int updatedAt = 100, int deleted = 0, double amount = 1}) => {
      'id': id,
      'medicine_id': medId,
      'taken_at': at,
      'amount': amount,
      'note': '',
      'updated_at': updatedAt,
      'deleted': deleted,
    };

void main() {
  group('库存与用量', () {
    test('半片用量：28 片每次半片每天一次 = 56 天', () {
      expect(med(remaining: 28, perBox: 28, doses: 1, amount: .5).daysLeft, closeTo(56, .001));
    });

    test('没设用量时算不出天数，也不算偏低', () {
      final m = med(doses: 0, amount: 0);
      expect(m.daysLeft, isNull);
      expect(m.isLow, isFalse);
    });

    test('偏低与用完是两个状态', () {
      expect(med(remaining: 5, doses: 1, amount: 1).isLow, isTrue); // 5 天 ≤ 7
      expect(med(remaining: 0).isOut, isTrue);
      expect(med(remaining: 0).isLow, isTrue, reason: '用完必然也低于阈值');
    });
  });

  group('服药时间点', () {
    test('times 为空时按次数自动生成', () {
      expect(med(doses: 1).scheduleMinutes, [8 * 60]);
      final three = med(doses: 3).scheduleMinutes;
      expect(three.first, 7 * 60 + 30);
      expect(three.last, 21 * 60);
      expect(three, orderedEquals([...three]..sort()));
    });

    test('times 解析并按时间排序，容忍中文分隔符', () {
      expect(med(times: '20:00，08:00; 14:00').scheduleMinutes, [8 * 60, 14 * 60, 20 * 60]);
    });

    test('非法时间点忽略后回退自动生成', () {
      expect(med(times: '99:99').scheduleMinutes, med().scheduleMinutes);
    });

    test('未设置用量时不生成时间点', () {
      expect(med(doses: 0).scheduleMinutes, isEmpty);
    });
  });

  group('槽位与打卡就近匹配', () {
    const base = 1759276800000;
    final slots = [8 * 60, 12 * 60, 20 * 60];
    int at(int hour, [int minute = 0]) => base + (hour * 60 + minute) * 60000;

    test('只打了晚上的卡，点亮 20:00 而不是 08:00', () {
      final m = matchSlots(slots, [at(20, 1)], base);
      expect(m[0], isNull);
      expect(m[2], at(20, 1));
    });

    test('迟到的 08:30 仍算早间那一格', () {
      final m = matchSlots(slots, [at(8, 30)], base);
      expect(m[0], at(8, 30));
      expect(m.sublist(1), [null, null]);
    });

    test('两次打卡按就近占据两格，剩下最远那格算漏', () {
      final m = matchSlots(slots, [at(9), at(13)], base);
      expect(m[0], at(9));
      expect(m[1], at(13));
      expect(m[2], isNull);
    });

    test('空槽位或没有打卡', () {
      expect(matchSlots(const [], [at(8)], base), isEmpty);
      expect(matchSlots(slots, const [], base), [null, null, null]);
    });
  });

  group('今日计划', () {
    // 2026-10-30 12:34 本地时间
    DateTime now() => DateTime(2026, 10, 30, 12, 34);
    int dayStart() => DateTime(2026, 10, 30).millisecondsSinceEpoch;

    test('已服 / 未到 / 待记 / 没吃分得清，分母扣掉没吃', () {
      final view = MedicineView(
        medicines: [
          med(id: 'a', name: '维生素D', times: '08:00'),
          med(id: 'b', name: '钙片', doses: 2, times: '08:00,20:00'),
          med(id: 'c', name: '降压药', times: '20:00'),
        ],
        logs: [
          log('l1', 'a', dayStart() + 8 * 3600000 + 5 * 60000),
          log('l2', 'b', dayStart() + 8 * 3600000 + 10 * 60000),
          // 20:00 那格提前标了「没吃」：0 剂量
          log('l3', 'c', dayStart() + 12 * 3600000, amount: 0),
        ],
        publishedAt: 1,
        devices: 1,
      );
      final plan = view.todayPlan(now());
      expect(plan.slots.length, 4);
      expect(plan.taken, 2);
      expect(plan.skipped, 1);
      expect(plan.planned, 3, reason: '分母扣掉「没吃」的格子');
      expect(plan.due, isNull, reason: '早上两格都记了');
      expect(plan.next?.minute, 20 * 60);
      expect(plan.allDone, isFalse);
    });

    test('过了 30 分钟还没记 = 待记，且排在「下一次」之前', () {
      final view = MedicineView(
        medicines: [med(times: '08:00')],
        logs: const [],
        publishedAt: 1,
        devices: 1,
      );
      final plan = view.todayPlan(now());
      expect(plan.due?.minute, 8 * 60);
      expect(plan.next, isNull, reason: '只有一格且已经过点');
    });

    test('全部记完说都记了；没有安排时计划为空', () {
      final done = MedicineView(
        medicines: [med(times: '08:00')],
        logs: [log('l1', 'm1', dayStart() + 8 * 3600000)],
        publishedAt: 1,
        devices: 1,
      ).todayPlan(now());
      expect(done.allDone, isTrue);

      final none = MedicineView(
        medicines: [med(doses: 0)],
        logs: const [],
        publishedAt: 1,
        devices: 1,
      ).todayPlan(now());
      expect(none.slots, isEmpty);
      expect(none.allDone, isFalse);
    });
  });

  group('多设备合并', () {
    test('同一行取 updated_at 更新的那份', () {
      final view = MedicineView.merge([
        MedicineDoc(device: 'phone-a', publishedAt: 100, medicines: [medRow('m1', updatedAt: 50, remaining: 1)], logs: const []),
        MedicineDoc(device: 'phone-b', publishedAt: 90, medicines: [medRow('m1', updatedAt: 60, remaining: 9)], logs: const []),
      ]);
      expect(view.medicines.single.remaining, 9);
      expect(view.publishedAt, 100, reason: '来源时间取最近一次发布');
      expect(view.devices, 2);
    });

    test('平票时按发布更晚、再按设备号，结果与读取顺序无关', () {
      final a = MedicineDoc(device: 'aaa', publishedAt: 100, medicines: [medRow('m1', updatedAt: 50, remaining: 1)], logs: const []);
      final b = MedicineDoc(device: 'bbb', publishedAt: 200, medicines: [medRow('m1', updatedAt: 50, remaining: 2)], logs: const []);
      expect(MedicineView.merge([a, b]).medicines.single.remaining, 2);
      expect(MedicineView.merge([b, a]).medicines.single.remaining, 2);
    });

    test('墓碑行不出现在视图里，其打卡也不显示', () {
      final view = MedicineView.merge([
        MedicineDoc(
          device: 'phone-a',
          publishedAt: 10,
          medicines: [medRow('m1', deleted: 1), medRow('m2')],
          logs: [logRow('l1', 'm1', 5), logRow('l2', 'm2', 6)],
        ),
      ]);
      expect(view.medicines.map((m) => m.id), ['m2']);
      expect(view.logs.map((l) => l.id), ['l2'], reason: '已删药品的打卡不该漏出来');
    });

    test('视图缓存往返不改内容', () {
      final view = MedicineView.merge([
        MedicineDoc(
          device: 'phone-a',
          publishedAt: 10,
          medicines: [medRow('m1', remaining: 12.5)],
          logs: [logRow('l1', 'm1', 6)],
        ),
      ], fetchedAt: 11);
      final back = MedicineView.fromJson(view.toJson());
      expect(back.medicines.single.remaining, 12.5);
      expect(back.logs.single.takenAt, 6);
      expect(back.publishedAt, 10);
      expect(back.fetchedAt, 11);
      expect(back.devices, 1);
    });
  });
}
