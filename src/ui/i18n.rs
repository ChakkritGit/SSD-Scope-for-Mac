//! English / Thai interface text. English strings are the lookup keys; anything without a
//! translation (drive names, numbers, paths) is shown unchanged.
use std::sync::atomic::{AtomicU8, Ordering};

static THAI: AtomicU8 = AtomicU8::new(0);

/// `choice`: 0 follows the system language, 1 English, 2 Thai.
pub fn set(choice: usize) {
    let thai = match choice {
        1 => false,
        2 => true,
        _ => system_is_thai(),
    };
    THAI.store(thai as u8, Ordering::Relaxed);
}

pub fn is_thai() -> bool {
    THAI.load(Ordering::Relaxed) == 1
}

fn system_is_thai() -> bool {
    std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.split(['"', ',', '(', ')', '\n', ' '])
                .find(|w| !w.is_empty())
                .map(|w| w.starts_with("th"))
        })
        .unwrap_or(false)
}

pub fn tr(text: &str) -> String {
    if is_thai()
        && let Some((_, thai)) = TABLE.iter().find(|(en, _)| *en == text)
    {
        return (*thai).to_owned();
    }
    text.to_owned()
}

/// Translate a template, then fill each `{}` in order.
pub fn trf<S: AsRef<str>>(template: &str, args: &[S]) -> String {
    let mut out = tr(template);
    for arg in args {
        if let Some(i) = out.find("{}") {
            out.replace_range(i..i + 2, arg.as_ref());
        }
    }
    out
}

const TABLE: &[(&str, &str)] = &[
    ("Dashboard", "ภาพรวม"),
    ("Health", "สุขภาพ"),
    ("Drive Information", "ข้อมูลไดรฟ์"),
    ("Model Name", "ชื่อรุ่น"),
    ("Capacity", "ความจุ"),
    ("Interface", "อินเทอร์เฟซ"),
    ("Location", "ตำแหน่ง"),
    ("Disk", "ดิสก์"),
    ("SMART data", "ข้อมูล SMART"),
    ("Available", "ว่าง"),
    ("Used", "ใช้แล้ว"),
    ("Total", "ทั้งหมด"),
    ("Lifetime", "อายุการใช้งาน"),
    ("Appearance", "รูปแบบสี"),
    ("Light", "สว่าง"),
    ("System", "ตามระบบ"),
    ("Dark", "มืด"),
    ("Performance", "ประสิทธิภาพ"),
    ("Backup", "สำรองข้อมูล"),
    ("Settings", "การตั้งค่า"),
    ("Drives", "ไดรฟ์"),
    ("for Mac", "สำหรับ Mac"),
    ("Your storage", "อุปกรณ์จัดเก็บข้อมูล"),
    ("Refresh", "รีเฟรช"),
    ("Export report", "ส่งออกรายงาน"),
    ("Refreshing…", "กำลังรีเฟรช…"),
    ("Cancel", "ยกเลิก"),
    ("Start", "เริ่ม"),
    ("Ready", "พร้อม"),
    ("Needs attention", "ต้องตรวจสอบ"),
    (
        "· Read-only monitoring · data stays on this Mac",
        "· ตรวจสอบแบบอ่านอย่างเดียว · ข้อมูลไม่ออกจาก Mac เครื่องนี้",
    ),
    ("Activity log", "บันทึกการทำงาน"),
    ("Dismiss", "ปิด"),
    ("Show backup in Finder", "แสดงข้อมูลสำรองใน Finder"),
    ("Show in Finder", "แสดงใน Finder"),
    ("Drive health", "สุขภาพไดรฟ์"),
    ("health", "สุขภาพ"),
    ("SMART failing", "SMART แจ้งว่าใกล้เสีย"),
    ("Beyond rated endurance", "เกินอายุการเขียนที่กำหนด"),
    ("Good", "ดี"),
    ("Fair", "พอใช้"),
    ("Low endurance", "อายุการใช้งานเหลือน้อย"),
    ("SMART passed", "SMART ผ่าน"),
    ("Health unknown", "ไม่ทราบสุขภาพ"),
    ("Internal", "ภายใน"),
    ("External", "ภายนอก"),
    ("SMART captured {}", "อ่าน SMART เมื่อ {}"),
    ("Back up this drive", "สำรองข้อมูลไดรฟ์นี้"),
    ("View details", "ดูรายละเอียด"),
    ("Temperature", "อุณหภูมิ"),
    ("not reported", "ไม่มีข้อมูล"),
    ("current", "ปัจจุบัน"),
    ("snapshot · {}", "ค่าที่อ่านไว้ · {}"),
    ("Power-on time", "เวลาเปิดใช้งาน"),
    ("≈ {} years", "≈ {} ปี"),
    ("Power cycles", "จำนวนครั้งที่เปิดเครื่อง"),
    ("{} unexpected", "ไฟดับกะทันหัน {} ครั้ง"),
    ("Data written", "ข้อมูลที่เขียน"),
    ("{} read", "อ่าน {}"),
    ("Live activity", "การทำงานสด"),
    ("Hover the chart for details", "ชี้เมาส์บนกราฟเพื่อดูรายละเอียด"),
    ("Storage", "พื้นที่จัดเก็บ"),
    ("No mounted volumes.", "ไม่มีวอลุ่มที่ mount อยู่"),
    ("{} used · {} free of {}", "ใช้ไป {} · ว่าง {} จาก {}"),
    ("Insights", "ข้อสังเกต"),
    ("Nothing needs your attention.", "ไม่มีอะไรต้องกังวล"),
    (
        "Flash cells have averaged {} erase cycles against a rating of {} ({}% of rated endurance). Transcend SSD Scope reports this as 0% health. The drive can keep working, but back up anything important.",
        "เซลล์แฟลชถูกลบ-เขียนเฉลี่ย {} รอบ จากที่รองรับ {} รอบ (ใช้ไป {}% ของอายุที่กำหนด) Transcend SSD Scope จะแสดงเป็นสุขภาพ 0% ไดรฟ์ยังใช้งานต่อได้ แต่ควรสำรองข้อมูลสำคัญไว้",
    ),
    (
        "{} of {} rated erase cycles used ({}%). Calculated the same way as Transcend SSD Scope.",
        "ใช้รอบลบ-เขียนไป {} จาก {} รอบ ({}%) คำนวณแบบเดียวกับ Transcend SSD Scope",
    ),
    ("{}% of rated endurance used.", "ใช้ไป {}% ของอายุที่กำหนด"),
    (
        "macOS cannot read SMART live through this USB bridge. Use Read SMART now on the Health page.",
        "macOS อ่าน SMART ผ่านชิป USB ตัวนี้แบบสดไม่ได้ กด “อ่าน SMART ตอนนี้” ในหน้าสุขภาพ",
    ),
    (
        "This drive did not report wear information.",
        "ไดรฟ์นี้ไม่รายงานข้อมูลการสึกหรอ",
    ),
    ("Rated endurance exceeded", "เกินอายุการเขียนที่กำหนด"),
    (
        "Keep a current backup and plan a replacement.",
        "ควรสำรองข้อมูลให้เป็นปัจจุบันและวางแผนเปลี่ยนไดรฟ์",
    ),
    ("{} link CRC errors", "ข้อผิดพลาด CRC ในการเชื่อมต่อ {} ครั้ง"),
    (
        "Usually the USB enclosure, cable or port, not flash wear. Try another cable or port.",
        "มักเกิดจากกล่อง USB สาย หรือพอร์ต ไม่ใช่การสึกหรอของแฟลช ลองเปลี่ยนสายหรือพอร์ต",
    ),
    ("Write amplification ≈ {}×", "อัตราการเขียนเพิ่ม (WAF) ≈ {}×"),
    (
        "The flash wrote far more than the computer sent, usually from many small random writes on a nearly full drive. Keeping more free space helps.",
        "แฟลชถูกเขียนมากกว่าที่คอมพิวเตอร์ส่งไปมาก มักเกิดจากการเขียนไฟล์เล็ก ๆ จำนวนมากบนไดรฟ์ที่ใกล้เต็ม การเว้นพื้นที่ว่างไว้มากขึ้นจะช่วยได้",
    ),
    ("Running hot ({} °C)", "ร้อนเกินไป ({} °C)"),
    (
        "Improve airflow around the enclosure.",
        "จัดให้กล่องไดรฟ์ระบายความร้อนได้ดีขึ้น",
    ),
    (
        "Values are from a saved SMART reading",
        "ค่าเหล่านี้มาจาก SMART ที่อ่านเก็บไว้",
    ),
    (
        "This USB bridge does not expose SMART to macOS while mounted. Use Read SMART now on the Health page to update.",
        "ชิป USB ตัวนี้ไม่ส่ง SMART ให้ macOS ขณะ mount อยู่ กด “อ่าน SMART ตอนนี้” ในหน้าสุขภาพเพื่ออัปเดต",
    ),
    ("endurance remaining", "อายุการใช้งานคงเหลือ"),
    ("Overall SMART", "สถานะ SMART รวม"),
    ("self-assessment", "การประเมินตัวเอง"),
    ("Spare blocks", "บล็อกสำรอง"),
    ("{} reallocated", "ย้ายบล็อกแล้ว {}"),
    ("Uncorrectable", "แก้ไขไม่ได้"),
    ("media errors", "ข้อผิดพลาดของสื่อ"),
    ("Endurance", "อายุการเขียน"),
    (
        "Average erase count vs. rated cycles",
        "จำนวนลบเฉลี่ยเทียบกับรอบที่กำหนด",
    ),
    ("{} average erases", "ลบเฉลี่ย {} รอบ"),
    (
        "· rated {} · marker = 100% of rating",
        "· กำหนด {} รอบ · เส้นตั้ง = 100% ของสเปก",
    ),
    (
        "Per-block range {} – {} (even wear levelling)",
        "ช่วงต่อบล็อก {} – {} (กระจายการสึกหรอสม่ำเสมอ)",
    ),
    (
        "Flash writes {} · host writes {}",
        "เขียนลงแฟลช {} · คอมพิวเตอร์เขียน {}",
    ),
    ("SMART attributes", "ค่า SMART"),
    ("Read {}", "อ่านเมื่อ {}"),
    (
        "{} attributes have thresholds, {} at or below. Hover a row for its meaning.",
        "มี {} ค่าที่มีเกณฑ์ และต่ำกว่าเกณฑ์ {} ค่า ชี้เมาส์ที่แถวเพื่อดูความหมาย",
    ),
    ("ID", "ID"),
    ("Attribute", "รายการ"),
    ("Value", "ค่า"),
    ("Worst", "แย่สุด"),
    ("Threshold", "เกณฑ์"),
    ("Raw", "ค่าดิบ"),
    ("Capture notes", "หมายเหตุการอ่าน"),
    ("SMART source", "แหล่งข้อมูล SMART"),
    (
        "macOS does not pass SMART commands through this USB bridge while the drive is mounted (Windows uses a kernel pass-through that macOS does not offer). Read SMART now briefly unmounts the drive, asks for your administrator password, reads SMART directly over USB, then mounts it again.",
        "macOS ไม่ส่งคำสั่ง SMART ผ่านชิป USB ตัวนี้ขณะไดรฟ์ mount อยู่ (Windows ใช้ช่องทางของเคอร์เนลที่ macOS ไม่มีให้) ปุ่ม “อ่าน SMART ตอนนี้” จะ unmount ไดรฟ์ชั่วคราว ขอรหัสผู้ดูแลระบบ อ่าน SMART ผ่าน USB โดยตรง แล้ว mount กลับให้",
    ),
    ("Read SMART now", "อ่าน SMART ตอนนี้"),
    ("Reading SMART from the drive…", "กำลังอ่าน SMART จากไดรฟ์…"),
    ("Import capture…", "นำเข้าไฟล์ที่อ่านไว้…"),
    ("Remove saved reading", "ลบค่าที่อ่านไว้"),
    ("Check live SMART", "ตรวจ SMART แบบสด"),
    (
        "Close apps and files using the drive first.",
        "ปิดแอปและไฟล์ที่ใช้ไดรฟ์นี้ก่อน",
    ),
    ("Read now", "อ่านขณะนี้"),
    ("Write now", "เขียนขณะนี้"),
    ("peak {}", "สูงสุด {}"),
    ("Connection", "การเชื่อมต่อ"),
    ("internal", "ภายใน"),
    ("external", "ภายนอก"),
    (
        "IORegistry counters · last 2 minutes",
        "จากตัวนับ IORegistry · 2 นาทีล่าสุด",
    ),
    ("Speed test", "ทดสอบความเร็ว"),
    ("256 MiB sequential", "อ่าน/เขียนต่อเนื่อง 256 MiB"),
    (
        "Writes a new temporary file in a folder you choose, reads it back and verifies it, then removes it. The OS cache is bypassed; the drive's own cache still affects results.",
        "เขียนไฟล์ชั่วคราวใหม่ในโฟลเดอร์ที่เลือก อ่านกลับและตรวจสอบ แล้วลบทิ้ง ไม่ผ่านแคชของระบบ แต่แคชของไดรฟ์ยังมีผลต่อผลลัพธ์",
    ),
    ("Write", "เขียน"),
    ("Read", "อ่าน"),
    ("Read + verify", "อ่าน + ตรวจสอบ"),
    ("Folder: {}", "โฟลเดอร์: {}"),
    ("Choose folder and run…", "เลือกโฟลเดอร์แล้วเริ่มทดสอบ…"),
    ("File system check", "ตรวจระบบไฟล์"),
    (
        "Checks the file system structure (not the flash cells). Access may pause briefly. Nothing is repaired.",
        "ตรวจโครงสร้างระบบไฟล์ (ไม่ใช่เซลล์แฟลช) อาจหยุดการเข้าถึงชั่วครู่ ไม่มีการซ่อมแซมใด ๆ",
    ),
    ("Verify {}", "ตรวจ {}"),
    (
        "Copy a folder or a whole external volume into a NEW backup folder. Existing files are never overwritten or merged. Not a bootable clone.",
        "คัดลอกโฟลเดอร์หรือทั้งวอลุ่มภายนอกไปยังโฟลเดอร์สำรองใหม่ ไม่เขียนทับหรือรวมกับไฟล์เดิม และไม่ใช่การโคลนเพื่อบูต",
    ),
    ("Source", "ต้นทาง"),
    ("Choose folder…", "เลือกโฟลเดอร์…"),
    ("Use {}", "ใช้ {}"),
    ("No source selected", "ยังไม่ได้เลือกต้นทาง"),
    ("Destination", "ปลายทาง"),
    ("Choose destination…", "เลือกปลายทาง…"),
    ("No destination selected", "ยังไม่ได้เลือกปลายทาง"),
    ("Verify and start", "ตรวจสอบและเริ่ม"),
    (
        "Verify every file with SHA-256 after copying",
        "ตรวจทุกไฟล์ด้วย SHA-256 หลังคัดลอก",
    ),
    (
        "Permissions, extended attributes and resource forks are copied by macOS ditto.",
        "สิทธิ์ไฟล์ แอตทริบิวต์เสริม และ resource fork คัดลอกด้วย ditto ของ macOS",
    ),
    ("Review and start backup…", "ตรวจทานแล้วเริ่มสำรอง…"),
    ("Cannot start", "เริ่มไม่ได้"),
    ("Restore from a Scope backup", "กู้คืนจากข้อมูลสำรองของ Scope"),
    (
        "Choose a completed Scope Backup folder; its Files folder becomes the source above. Then pick a new destination.",
        "เลือกโฟลเดอร์ Scope Backup ที่เสร็จแล้ว โฟลเดอร์ Files จะกลายเป็นต้นทางด้านบน จากนั้นเลือกปลายทางใหม่",
    ),
    ("Choose completed backup…", "เลือกข้อมูลสำรองที่เสร็จแล้ว…"),
    ("Language", "ภาษา"),
    ("Automatic", "อัตโนมัติ"),
    ("Menu bar", "แถบเมนู"),
    ("Saved automatically", "บันทึกอัตโนมัติ"),
    ("Show SSD Scope in the menu bar", "แสดง SSD Scope บนแถบเมนู"),
    ("Show a value next to the icon", "แสดงค่าข้างไอคอน"),
    ("Free space", "พื้นที่ว่าง"),
    ("Read / write", "อ่าน / เขียน"),
    (
        "Keep running in the menu bar when the window closes",
        "ทำงานต่อบนแถบเมนูเมื่อปิดหน้าต่าง",
    ),
    (
        "Temperature from a saved SMART reading is labelled with its reading time.",
        "อุณหภูมิจาก SMART ที่อ่านไว้จะแสดงพร้อมเวลาที่อ่าน",
    ),
    ("About", "เกี่ยวกับ"),
    (
        "Independent open-source utility (MIT). Not affiliated with or endorsed by Transcend. Health follows the formula used by Transcend SSD Scope for Windows: (rated erase cycles − average erase count) ÷ rated erase cycles.",
        "ยูทิลิตีโอเพนซอร์สอิสระ (MIT) ไม่มีส่วนเกี่ยวข้องหรือได้รับการรับรองจาก Transcend ค่าสุขภาพใช้สูตรเดียวกับ Transcend SSD Scope บน Windows: (รอบลบที่กำหนด − รอบลบเฉลี่ย) ÷ รอบลบที่กำหนด",
    ),
    ("Source code on GitHub", "ซอร์สโค้ดบน GitHub"),
    ("Run speed test?", "เริ่มทดสอบความเร็ว?"),
    ("Start file backup?", "เริ่มสำรองไฟล์?"),
    ("Collecting disk activity…", "กำลังเก็บข้อมูลการทำงานของดิสก์…"),
    ("2 min ago", "2 นาทีก่อน"),
    ("1 min ago", "1 นาทีก่อน"),
    ("now", "ตอนนี้"),
    ("Now", "ตอนนี้"),
    ("{} s ago", "{} วินาทีก่อน"),
    ("{} min ago", "{} นาทีก่อน"),
    ("{} h ago", "{} ชั่วโมงก่อน"),
    ("{} days ago", "{} วันก่อน"),
    ("Reading drive information…", "กำลังอ่านข้อมูลไดรฟ์…"),
    (
        "No drives could be read. Press Refresh. If macOS denies disk access, allow it in System Settings → Privacy & Security.",
        "อ่านข้อมูลไดรฟ์ไม่ได้ กดรีเฟรช ถ้า macOS ไม่อนุญาตให้เข้าถึงดิสก์ ให้อนุญาตใน การตั้งค่าระบบ → ความเป็นส่วนตัวและความปลอดภัย",
    ),
    ("unavailable", "ไม่มีข้อมูล"),
    ("Importing USB SMART snapshot…", "กำลังนำเข้า SMART…"),
    ("Removing saved snapshot…", "กำลังลบค่าที่อ่านไว้…"),
    ("Reading SMART…", "กำลังอ่าน SMART…"),
    ("Verifying file system…", "กำลังตรวจระบบไฟล์…"),
    (
        "Writing, reading and verifying test file…",
        "กำลังเขียน อ่าน และตรวจสอบไฟล์ทดสอบ…",
    ),
    (
        "Copying files, then verifying SHA-256…",
        "กำลังคัดลอกไฟล์ แล้วตรวจ SHA-256…",
    ),
    ("English", "English"),
    ("About SSD Scope", "เกี่ยวกับ SSD Scope"),
    ("Settings…", "การตั้งค่า…"),
    ("Hide SSD Scope", "ซ่อน SSD Scope"),
    ("Quit SSD Scope", "ออกจาก SSD Scope"),
    ("File", "ไฟล์"),
    ("View", "มุมมอง"),
    ("Window", "หน้าต่าง"),
    ("Help", "วิธีใช้"),
    ("Refresh drives", "รีเฟรชไดรฟ์"),
    ("Close window", "ปิดหน้าต่าง"),
    ("Zoom", "ย่อ/ขยาย"),
    ("Open dashboard", "เปิดหน้าภาพรวม"),
    ("USB SMART support", "การรองรับ SMART ผ่าน USB"),
    ("Project on GitHub", "โปรเจกต์บน GitHub"),
    ("Reading drives…", "กำลังอ่านไดรฟ์…"),
    ("SMART: waiting", "SMART: กำลังรอ"),
    ("SMART capture: none", "SMART ที่อ่านไว้: ไม่มี"),
    ("No connected drive", "ไม่มีไดรฟ์ที่เชื่อมต่อ"),
    ("Capacity unavailable", "ไม่ทราบความจุ"),
    ("SMART: unavailable", "SMART: ไม่มีข้อมูล"),
    ("{} free of {}", "ว่าง {} จาก {}"),
    ("SMART: {}", "SMART: {}"),
    ("USB transport unavailable", "อ่านผ่าน USB ไม่ได้"),
    ("Access denied", "ไม่ได้รับอนุญาต"),
    ("Unavailable", "ไม่มีข้อมูล"),
    ("Unknown", "ไม่ทราบ"),
    ("Verified", "ผ่าน"),
    ("Failing", "ใกล้เสีย"),
    ("Not Supported", "ไม่รองรับ"),
];
