use std::{fs, io, path::Path};

use pluginapi::{exdll_init, popstring, pushint, stack_t, wchar_t};
use progress_streams::ProgressReader;
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Controls::{PBM_SETPOS, PROGRESS_CLASSW, WC_STATICW},
        WindowsAndMessaging::{
            CreateWindowExW, FindWindowExW, GetWindowLongPtrW, SendMessageW, SetWindowPos,
            SetWindowTextW, GWL_STYLE, SWP_FRAMECHANGED, SWP_NOSIZE, WM_GETFONT, WM_SETFONT,
            WS_CHILD, WS_VISIBLE,
        },
    },
};

/// Download a file from an URL to a path.
///
/// # Safety
///
/// This function always expects 2 strings on the stack ($1: url, $2: path) and will panic otherwise.
#[no_mangle]
pub unsafe extern "C" fn Download(
    hwnd_parent: HWND,
    string_size: u32,
    variables: *mut wchar_t,
    stacktop: *mut *mut stack_t,
) {
    exdll_init(string_size, variables, stacktop);

    let url = popstring().unwrap();
    let path = popstring().unwrap();

    let status = download_file(hwnd_parent, &url, &path);
    pushint(status);
}

fn download_file(hwnd_parent: HWND, url: &str, path: &str) -> i32 {
    let mut childwnd = 0;
    let mut progress_bar = None;
    let mut progress_text = None;
    let mut downloading_text = None;
    let mut details_section = None;
    let mut details_section_resized = false;

    if hwnd_parent != 0 {
        childwnd = unsafe {
            let class = pluginapi::encode_wide("#32770");
            FindWindowExW(hwnd_parent, 0, class.as_ptr(), std::ptr::null())
        };

        if childwnd != 0 {
            unsafe {
                progress_bar = Some(CreateWindowExW(
                    0,
                    PROGRESS_CLASSW,
                    std::ptr::null(),
                    WS_CHILD | WS_VISIBLE,
                    0,
                    75,
                    450,
                    18,
                    childwnd,
                    0,
                    0,
                    std::ptr::null(),
                ));

                downloading_text = Some(CreateWindowExW(
                    0,
                    WC_STATICW,
                    std::ptr::null(),
                    WS_CHILD | WS_VISIBLE,
                    0,
                    95,
                    450,
                    18,
                    childwnd,
                    0,
                    0,
                    std::ptr::null(),
                ));

                progress_text = Some(CreateWindowExW(
                    0,
                    WC_STATICW,
                    std::ptr::null(),
                    WS_CHILD | WS_VISIBLE,
                    0,
                    113,
                    450,
                    18,
                    childwnd,
                    0,
                    0,
                    std::ptr::null(),
                ));

                let font = SendMessageW(childwnd, WM_GETFONT, 0, 0);
                SendMessageW(downloading_text.unwrap(), WM_SETFONT, font as _, 0);
                SendMessageW(progress_text.unwrap(), WM_SETFONT, font as _, 0);
            };
        }
    }

    let response = match ureq::get(url).call() {
        Ok(data) => data,
        Err(err) => {
            return match err {
                ureq::Error::Status(code, _) => code as i32,
                ureq::Error::Transport(_) => 499,
            }
        }
    };

    let total = response
        .header("Content-Length")
        .unwrap_or("0")
        .parse::<u128>()
        .unwrap();

    let mut read = 0;

    let mut reader = response.into_reader();
    let mut reader = ProgressReader::new(&mut reader, |progress: usize| {
        let details_section = details_section.unwrap_or_else(|| unsafe {
            let class = pluginapi::encode_wide("SysListView32");
            let section = FindWindowExW(childwnd, 0, class.as_ptr(), std::ptr::null());
            if section != 0 {
                details_section = Some(section);
            }
            section
        });

        if details_section != 0 {
            unsafe {
                let style = GetWindowLongPtrW(details_section, GWL_STYLE);
                let visible = (style & !WS_VISIBLE as i32) != style;

                if visible && !details_section_resized {
                    SetWindowPos(progress_bar.unwrap(), 0, 0, 40, 0, 0, SWP_NOSIZE);
                    SetWindowPos(downloading_text.unwrap(), 0, 0, 60, 0, 0, SWP_NOSIZE);
                    SetWindowPos(progress_text.unwrap(), 0, 0, 78, 0, 0, SWP_NOSIZE);
                    SetWindowPos(details_section, 0, 0, 100, 450, 120, SWP_FRAMECHANGED);

                    details_section_resized = true;
                }
            }
        }

        read += progress;
        let percentage = (read as f64 / total as f64) * 100.0;

        if let Some(progress_bar) = progress_bar {
            unsafe { SendMessageW(progress_bar, PBM_SETPOS, percentage as _, 0) };
        }

        if let Some(progress_text) = progress_text {
            let text = pluginapi::encode_wide(format!(
                "{} / {} KiB  - {:.2}%",
                read / 1024,
                total / 1024,
                percentage,
            ));
            unsafe { SetWindowTextW(progress_text, text.as_ptr()) };

            let text = pluginapi::encode_wide(format!("Downloading {} ...", url));
            unsafe { SetWindowTextW(downloading_text.unwrap(), text.as_ptr()) };
        }

        if percentage >= 100. {
            unsafe {
                SetWindowPos(details_section, 0, 0, 41, 450, 180, SWP_FRAMECHANGED);
            }
        }
    });

    let path = Path::new(path);
    fs::create_dir_all(path.parent().unwrap_or_else(|| Path::new("."))).unwrap();

    let mut file = fs::File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap();

    let res = io::copy(&mut reader, &mut file);

    i32::from(res.is_err())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_downloads() {
        assert_eq!(
            download_file(
                0,
                "https://go.microsoft.com/fwlink/p/?LinkId=2124703",
                "wv2setup.exe"
            ),
            0
        )
    }
}
