/// 演示数据 —— 与 `design/workbench-mobile-ui.html` 那份**同一批假数据**。
///
/// 为什么要冻一份假数据在仓库里：M11c 的验收是「360×800dp 逐屏截图核对」，
/// 而截图要能跟他已经拍板的那四版对比稿逐像素对照，时钟与内容都必须钉住
/// （桌面 M9c 冻时钟对句是同一条做法）。跑真数据时用不带 ANVIL_DEMO 的那条路。
///
/// 这里的课程名、待办标题全是编的，不含任何真实个人信息。
library;

import 'medicine.dart';
import 'store.dart';
import 'timetable.dart';
import 'todo.dart';

/// 对比稿的「现在」：2026-10-30 周五 12:34，第 9 周。
Clock demoClock() => Clock.demo(2026, 10, 30, 12, 34);

/// 第 9 周的那个周一往回推 8 周
const _week1Monday = '2026-08-31';
const _totalWeeks = 18;

/// 五个大节（对比稿的 PERIODS）。骨架由课表行推出来，所以每天要有课落在这些节次上，
/// 空着的那些才会以「空闲」出现 —— 而不是整行消失。
const _slots = [
  (label: '第一大节 [01-02]节', start: 480, end: 580), // 08:00–09:40
  (label: '第二大节 [03-04]节', start: 600, end: 700), // 10:00–11:40
  (label: '第三大节 [05-06]节', start: 840, end: 940), // 14:00–15:40
  (label: '第四大节 [07-08]节', start: 960, end: 1060), // 16:00–17:40
  (label: '第五大节 [09-10]节', start: 1140, end: 1240), // 19:00–20:40
];

/// 周五那天：第一、三大节有课，第五大节也有一节。
const _friday = [
  (1, '高等数学', '李老师', '3教 201'),
  (3, '数据结构与算法', '王老师', '实验中心 B302'),
  (5, '大学英语', '张老师', '外院楼 214'),
];

List<ClassRow> _demoClasses() {
  final rows = <ClassRow>[];
  var i = 0;
  for (final (slotN, name, teacher, room) in _friday) {
    final s = _slots[slotN - 1];
    rows.add(ClassRow(
      id: 'demo-c${i++}',
      term: '2026-2027-1',
      name: name,
      teacher: teacher,
      room: room,
      day: 5,
      startMin: s.start,
      endMin: s.end,
      period: s.label,
      weeks: '1-16',
    ));
  }
  // 周一到周四各塞一课，让 7 日条上那五个「有课的日子」的蓝点是真的，
  // 也让第二、四大节进了整学期骨架 —— 周五那两节空档才显得是空档而不是缺数据。
  for (final (day, slotN, name, teacher, room) in [
    (1, 2, '线性代数', '赵老师', '3教 105'),
    (2, 4, '计算机网络', '孙老师', '信息楼 210'),
    (3, 1, '高等数学', '李老师', '3教 201'),
    (4, 3, '数据结构与算法', '王老师', '实验中心 B302'),
  ]) {
    final s = _slots[slotN - 1];
    rows.add(ClassRow(
      id: 'demo-c${i++}',
      term: '2026-2027-1',
      name: name,
      teacher: teacher,
      room: room,
      day: day,
      startMin: s.start,
      endMin: s.end,
      period: s.label,
      weeks: '1-16',
    ));
  }
  return rows;
}

int _ms(int h, int mi, {int dayOffset = 0}) {
  final base = demoClock().date.add(Duration(days: dayOffset));
  return DateTime(base.year, base.month, base.day, h, mi).millisecondsSinceEpoch;
}

List<Todo> _demoTodos() {
  var i = 0;
  Todo t(String title,
      {int? due,
      int pri = 2,
      bool done = false,
      bool unsent = false,
      int? doneAt,
      String zone = 'inbox',
      double? key}) {
    i++;
    return Todo(
      id: 'demo-t$i',
      title: title,
      createdAt: _ms(8, 0, dayOffset: -2),
      createdBy: 'pc-a',
      updatedAt: _ms(9, 0),
      updatedBy: unsent ? 'phone-a' : 'pc-a',
      dueAt: due,
      priority: pri,
      sortKey: key ?? i * 1024,
      completed: done,
      doneAt: doneAt,
      dirty: unsent,
      zone: zone,
    );
  }

  return [
    // 逾期：一条昨天到期、一条四天前到期（对比稿的 sub 就是这两句）
    t('回房东的消息', due: _ms(18, 0, dayOffset: -1), pri: 1),
    t('把统计页的选型定下来', due: _ms(18, 0, dayOffset: -4), pri: 1),
    // 今天：两条带着「还没传出去」的黄点
    t('交实验报告', due: _ms(18, 0), pri: 1, unsent: true),
    t('买猫粮', due: _ms(20, 0), pri: 2, unsent: true),
    // 对比稿里那两条只写「今天」的：真实存储里就是没有截止时间、被用户自己放进
    // today 区的那类（桌面 todos::focus 的判据），不是界面替它编了个 00:00
    t('跑一次备份还原演练', pri: 2, zone: 'today'),
    t('给课表补第 9 周的作业', zone: 'today'),
    // 随时：没有截止时间，也没进 today 区
    t('整理旧笔记的标签'),
    t('看 WakeUp 怎么切周'),
    // 已完成
    t('倒垃圾', done: true, doneAt: _ms(11, 20)),
    t('预约牙科', done: true, doneAt: _ms(16, 5, dayOffset: -1)),
  ];
}

/// `--dart-define=ANVIL_DEMO=` 的取值。空串 = 不演示，走真数据（现在即空库）。
enum DemoCase {
  none,
  full,
  undo,
  empty,
  firstRun,
  offline,
  med;

  static DemoCase from(String raw) => switch (raw) {
        'full' => DemoCase.full,
        'undo' => DemoCase.undo,
        'empty' => DemoCase.empty,
        'first' => DemoCase.firstRun,
        'offline' => DemoCase.offline,
        'med' => DemoCase.med,
        _ => DemoCase.none,
      };
}

/// 药箱演示数据（编的，不含真实用药信息）：早上两粒已记，晚上还有两格；
/// 降压药已用完、维生素D 剩 2 天 —— 卡片上三种状态各一个。
MedicineView demoMedicineView() => MedicineView(
      publishedAt: _at(demoClock(), 12, 3),
      fetchedAt: _at(demoClock(), 12, 34),
      devices: 1,
      medicines: [
        Medicine(
          id: 'demo-m1',
          name: '维生素D',
          perBox: 30,
          remaining: 2,
          dosesPerDay: 1,
          amountPerDose: 1,
          unit: '粒',
          lowDays: 7,
          note: '',
          times: '08:00',
          createdAt: _ms(8, 0, dayOffset: -20),
          updatedAt: _ms(8, 0, dayOffset: -20),
          deleted: false,
        ),
        Medicine(
          id: 'demo-m2',
          name: '钙片',
          perBox: 60,
          remaining: 30,
          dosesPerDay: 2,
          amountPerDose: 1,
          unit: '片',
          lowDays: 7,
          note: '',
          times: '08:00,20:00',
          createdAt: _ms(8, 0, dayOffset: -20),
          updatedAt: _ms(8, 0, dayOffset: -20),
          deleted: false,
        ),
        Medicine(
          id: 'demo-m3',
          name: '降压药',
          perBox: 28,
          remaining: 0,
          dosesPerDay: 1,
          amountPerDose: 1,
          unit: '片',
          lowDays: 7,
          note: '',
          times: '20:00',
          createdAt: _ms(8, 0, dayOffset: -20),
          updatedAt: _ms(8, 0, dayOffset: -20),
          deleted: false,
        ),
      ],
      logs: [
        ..._weekBackfill(),
        DoseLog(
          id: 'demo-l1',
          medicineId: 'demo-m1',
          takenAt: _ms(8, 5),
          amount: 1,
          note: '',
          updatedAt: _ms(8, 5),
          deleted: false,
        ),
        DoseLog(
          id: 'demo-l2',
          medicineId: 'demo-m2',
          takenAt: _ms(8, 10),
          amount: 1,
          note: '',
          updatedAt: _ms(8, 10),
          deleted: false,
        ),
      ],
    );

/// 近 6 天的打卡补齐，只留两晚的降压药没打（前天与大前天）。
///
/// 为什么补：三环卡里「本周依从」看的就是这一周，演示数据只有今天两条的话
/// 拍出来永远是 8%，那张图什么都说明不了。补齐后是 92% / 连续 7 天，
/// 和真机上周期的形状一致 —— 数字仍由 `weekStats` 现算，不是写死在界面上的。
List<DoseLog> _weekBackfill() {
  const slots = [
    ('demo-m1', 8, 5),
    ('demo-m2', 8, 12),
    ('demo-m2', 20, 3),
    ('demo-m3', 20, 8),
  ];
  const missed = {2, 4}; // 漏掉晚上那格的那几天
  final out = <DoseLog>[];
  for (var back = 6; back >= 1; back--) {
    for (final (med, h, mi) in slots) {
      if (med == 'demo-m3' && missed.contains(back)) continue;
      final at = _ms(h, mi, dayOffset: -back);
      out.add(DoseLog(
        id: 'demo-w$back-$med',
        medicineId: med,
        takenAt: at,
        amount: 1,
        note: '',
        updatedAt: at,
        deleted: false,
      ));
    }
  }
  return out;
}

Store demoStore(DemoCase c) {
  final clock = demoClock();
  final lastSync = _at(clock, 12, 3);
  return switch (c) {
    // 真空态：有课、同步过、但一条待办都没有 → 那句必须带同步时间（§5.5 第三行）
    DemoCase.empty => Store(
        clock: clock,
        classes: _demoClasses(),
        week1Monday: _week1Monday,
        totalWeeks: _totalWeeks,
        sync: SyncedAt(lastSync),
        accountSaved: true,
      ),
    // 首次装好、一次都没成功过：与上一句是两句话
    DemoCase.firstRun => Store(clock: clock, accountSaved: false),
    // 没连上：不伪装实时，明说手上这份旧到什么时候
    DemoCase.offline => Store(
        clock: clock,
        todos: _demoTodos(),
        classes: _demoClasses(),
        week1Monday: _week1Monday,
        totalWeeks: _totalWeeks,
        sync: OfflineAt(lastSync),
        accountSaved: true,
      ),
    // 满数据（undo 那一档只是让撤销条先亮着，见 TodayScreen 的 initialUndo）
    DemoCase.full || DemoCase.undo => Store(
        clock: clock,
        todos: _demoTodos(),
        classes: _demoClasses(),
        week1Monday: _week1Monday,
        totalWeeks: _totalWeeks,
        sync: SyncedAt(lastSync),
        accountSaved: true,
      ),
    // 不演示 = 不带假数据。真机那条路根本不经过这里（main 会开库）。
    DemoCase.none => Store(clock: clock),
    // 药箱只读卡：待办与课表仍是满数据，额外挂一份用药进度
    DemoCase.med => Store(
        clock: clock,
        todos: _demoTodos(),
        classes: _demoClasses(),
        week1Monday: _week1Monday,
        totalWeeks: _totalWeeks,
        sync: SyncedAt(lastSync),
        accountSaved: true,
        medicine: demoMedicineView(),
      ),
  };
}

/// 演示数据里那句「上次同步于 12:03」——「2 条待上传」不写在这里，
/// 它由 `Store.pending` 从 dirty 数出来，与 §3.4 那句「计数来源唯一」同一口径。
int _at(Clock clock, int h, int mi) {
  final d = clock.date;
  return DateTime(d.year, d.month, d.day, h, mi).millisecondsSinceEpoch;
}

/// 撤销条演示用的那一条标题（对比稿画的是刚勾掉「倒垃圾」那一瞬间）
const undoDemoTitle = '倒垃圾';
