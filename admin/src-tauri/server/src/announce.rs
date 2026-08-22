//! 语音播报：单工作线程 + FIFO 队列，前一条完整播完才播下一条。
//!
//! 播报规则：
//!   下单：把机器名拆成字母/数字逐个播（0-9.wav A-Z.wav），最后播 order.wav；
//!         机器名里播不出来的字符（中文/符号）跳过；一个有效字符都没有时改播 message.wav。
//!   呼叫网管：直接播 call.wav；同一台机器 30 秒内重复呼叫只播一次（防刷屏）。
//!
//! Windows 下用 winmm PlaySoundW 同步播放；其他平台（开发/测试机）退化为日志。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Order,
    Call,
}

#[derive(Debug)]
struct Msg {
    machine: String,
    kind: Kind,
}

#[derive(Clone)]
pub struct Announcer {
    tx: Sender<Msg>,
}

impl Announcer {
    pub fn spawn(sound_dir: PathBuf) -> Announcer {
        // 无界队列：原则是所有播报都必须播（mpsc 无界，send 不阻塞、不丢）。
        let (tx, rx) = channel::<Msg>();
        std::thread::spawn(move || {
            let mut last_call: HashMap<String, Instant> = HashMap::new();
            while let Ok(msg) = rx.recv() {
                if msg.kind == Kind::Call {
                    let now = Instant::now();
                    // 只保留 30s 内叫过的机台：键是任意 ≤64 字符机器名，不清会随 24h 无限累积
                    last_call.retain(|_, t| now.duration_since(*t) < Duration::from_secs(30));
                    if let Some(t) = last_call.get(&msg.machine) {
                        if now.duration_since(*t) < Duration::from_secs(30) {
                            continue; // 30秒内重复呼叫，跳过
                        }
                    }
                    last_call.insert(msg.machine.clone(), now);
                }
                let playlist = build_playlist(&sound_dir, &msg.machine, msg.kind);
                for f in playlist {
                    play_wav(&f);
                }
            }
        });
        Announcer { tx }
    }

    pub fn announce(&self, machine: &str, kind: Kind) {
        // send（无界队列）：所有播报都必须播，绝不丢——即使播报来不及，也只是在内存里排队等
        let _ = self.tx.send(Msg { machine: machine.to_string(), kind });
    }
}

/// 纯函数：由机器名+事件类型生成 wav 播放列表（跳过磁盘上不存在的文件）。
pub fn build_playlist(sound_dir: &Path, machine: &str, kind: Kind) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |name: String| {
        let p = sound_dir.join(&name);
        if p.is_file() {
            out.push(p);
        }
    };
    // 把机器名拆成字母/数字逐个播（中文/符号跳过）；返回是否播出了有效字符
    let mut spell = |machine: &str| {
        let mut any = false;
        for ch in machine.chars() {
            let up = ch.to_ascii_uppercase();
            if up.is_ascii_alphanumeric() {
                any = true;
                push(format!("{up}.wav"));
            }
        }
        any
    };
    match kind {
        // 呼叫网管：先播机台号，再播 "…号机呼叫网管"；机器名全是中文就只播 call.wav
        Kind::Call => {
            spell(machine);
            push("call.wav".to_string());
        }
        Kind::Order => {
            if !spell(machine) {
                push("message.wav".to_string());
            }
            push("order.wav".to_string());
        }
    }
    out
}

#[cfg(windows)]
fn play_wav(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        // SND_FILENAME | SND_SYNC：同步播完才返回，保证队列顺序
        winapi::um::playsoundapi::PlaySoundW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            winapi::um::playsoundapi::SND_FILENAME | winapi::um::playsoundapi::SND_SYNC,
        );
    }
}

#[cfg(not(windows))]
fn play_wav(path: &Path) {
    // 开发/测试平台：不出声，按文件名模拟短暂耗时，保持队列语义
    eprintln!("[announce] {:?}", path.file_name().unwrap_or_default());
    std::thread::sleep(Duration::from_millis(20));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playlist_order_ascii_machine() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["P.wav", "C.wav", "0.wav", "8.wav", "order.wav"] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let list = build_playlist(dir.path(), "pc-08", Kind::Order);
        let names: Vec<_> = list.iter().map(|p| p.file_name().unwrap().to_str().unwrap().to_string()).collect();
        assert_eq!(names, vec!["P.wav", "C.wav", "0.wav", "8.wav", "order.wav"]);
    }

    #[test]
    fn playlist_skips_non_ascii_and_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["A.wav", "order.wav"] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        // 中文和符号跳过；B.wav 磁盘上不存在也跳过
        let list = build_playlist(dir.path(), "吧-A#B", Kind::Order);
        let names: Vec<_> = list.iter().map(|p| p.file_name().unwrap().to_str().unwrap().to_string()).collect();
        assert_eq!(names, vec!["A.wav", "order.wav"]);
    }

    #[test]
    fn playlist_fallback_when_no_valid_char() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["message.wav", "order.wav"] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let list = build_playlist(dir.path(), "网吧一号机", Kind::Order);
        let names: Vec<_> = list.iter().map(|p| p.file_name().unwrap().to_str().unwrap().to_string()).collect();
        assert_eq!(names, vec!["message.wav", "order.wav"]);
    }

    #[test]
    fn playlist_call() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["P.wav", "C.wav", "0.wav", "1.wav", "call.wav"] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        let list = build_playlist(dir.path(), "PC-01", Kind::Call);
        let names: Vec<_> = list.iter().map(|p| p.file_name().unwrap().to_str().unwrap().to_string()).collect();
        assert_eq!(names, vec!["P.wav", "C.wav", "0.wav", "1.wav", "call.wav"]);
    }
}
