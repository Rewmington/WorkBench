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
  int createdAt = 1,
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
      createdAt: createdAt,
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

Map<String, Object?> medRow(String id, {int updatedAt = 100, double remaining = 10, int deleted = 0, String name = '药', double doses = 1.0}) => {
      'id': id,
      'name': name,
      'per_box': 30.0,
      'remaining': remaining,
      'doses_per_day': doses,
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

    test('损坏的次数与 times 不能把手机格子数撑爆', () {
      // 与桌面 medicine_math 的同名测试一条尺子：数据来自外部程序，先限量再分配。
      expect(med(doses: 1e9).scheduleMinutes.length, maxScheduleSlots);
      expect(med(doses: double.infinity).scheduleMinutes.length, maxScheduleSlots);
      expect(med(doses: double.negativeInfinity).scheduleMinutes, isEmpty);
      expect(med(doses: double.nan).scheduleMinutes, isEmpty);

      final crowded = List.generate(
        500,
        (i) => '${(i ~/ 60 % 24).toString().padLeft(2, '0')}:${(i % 60).toString().padLeft(2, '0')}',
      ).join(',');
      expect(med(times: crowded, doses: 1e9).scheduleMinutes.length, maxScheduleSlots);
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

  group('本周依从与连续打卡', () {
    // 2026-10-30 12:34 本地时间；`at(0)` 是今天 00:00，`at(1)` 是昨天 00:00
    final today = DateTime(2026, 10, 30);
    DateTime now() => today.add(const Duration(hours: 12, minutes: 34));
    int at(int back, [int hour = 8]) =>
        today.subtract(Duration(days: back)).millisecondsSinceEpoch + hour * 3600000;

    MedicineView viewOf({required List<Medicine> meds, required List<DoseLog> logs}) =>
        MedicineView(medicines: meds, logs: logs, publishedAt: 1, devices: 1);

    test('没到的格子不算欠，同一天多打一次也不把依从率刷过 100%', () {
      final v = viewOf(meds: [
        med(times: '08:00,20:00', doses: 2, createdAt: at(1))
      ], logs: [
        log('l1', 'm1', at(1, 8)),
        log('l2', 'm1', at(1, 20)),
        log('l3', 'm1', at(0, 8)),
        log('l4', 'm1', at(0, 9)), // 早上补打了一次
      ]);
      final s = v.weekStats(now());
      expect([s.taken, s.owed], [3, 3], reason: '昨天两格 + 今天已过点那一格');
      expect(s.rate, 1.0);
      expect(s.hasPlan, isTrue);
    });

    test('标了「这顿没吃」不进分母，但连续打卡照断', () {
      final v = viewOf(meds: [
        med(times: '21:00', createdAt: at(1))
      ], logs: [log('l1', 'm1', at(1, 21), amount: 0)]);
      final s = v.weekStats(DateTime(2026, 10, 30, 20));
      expect([s.taken, s.owed, s.skipped], [0, 0, 1]);
      expect(s.rate, 1.0, reason: '整窗只有一格且标了没吃＝没有欠，药箱按满算');
      expect(v.streakDays(now()), 0, reason: '「没吃」不是打卡');
    });

    test('压根没有安排，不说自己是 100% 依从', () {
      final s = viewOf(meds: [med(doses: 0)], logs: const []).weekStats(now());
      expect(s.hasPlan, isFalse);
      expect(s.rate, 0.0);
    });

    test('今天还没打不算断，但也不提前 +1', () {
      final two = viewOf(meds: [med(times: '08:00')], logs: [
        log('l1', 'm1', at(1, 8)),
        log('l2', 'm1', at(2, 8)),
      ]);
      expect(two.streakDays(now()), 2);

      final three = viewOf(meds: [med(times: '08:00')], logs: [
        log('l1', 'm1', at(0, 8)),
        log('l2', 'm1', at(1, 8)),
        log('l3', 'm1', at(2, 8)),
      ]);
      expect(three.streakDays(now()), 3);

      final gapped = viewOf(meds: [med(times: '08:00')], logs: [
        log('l1', 'm1', at(0, 8)),
        log('l2', 'm1', at(2, 8)),
      ]);
      expect(gapped.streakDays(now()), 1);

      final stale = viewOf(meds: [med(times: '08:00')], logs: [
        log('l1', 'm1', at(0, 8)),
        log('l2', 'm1', at(9, 8)), // 窗口外
      ]);
      expect(stale.streakDays(now()), 1, reason: '9 天前那条不该折进连续天数');
    });
  });

  group('多设备合并', () {
    test('清单按还剩几天升序，算不出天数的沉底', () {
      final view = MedicineView.merge([
        MedicineDoc(device: 'pc', publishedAt: 1, medicines: [
          medRow('m1', name: '钙片', remaining: 60),
          medRow('m2', name: '益生菌', remaining: 5, doses: 0),
          medRow('m3', name: '二甲双胍', remaining: 3),
        ], logs: const []),
      ]);
      expect(view.byStock.map((m) => m.name), ['二甲双胍', '钙片', '益生菌']);

      // 排的是渲染那一份：进来的顺序换了不该换答案，而原清单本身不动
      final reversed = MedicineView(
        medicines: view.medicines.reversed.toList(),
        logs: const [],
        publishedAt: 1,
        devices: 1,
      );
      expect(reversed.byStock.map((m) => m.name), ['二甲双胍', '钙片', '益生菌']);
      expect(reversed.medicines.map((m) => m.name), ['二甲双胍', '益生菌', '钙片'],
          reason: '原清单没被就地排掉：排的是渲染那一份');
    });

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
