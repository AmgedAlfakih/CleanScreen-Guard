use anyhow::Result;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, AC_SRC_ALPHA,
    AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP,
    HDC,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, RegisterClassW,
    SetWindowDisplayAffinity, ShowWindow, UpdateLayeredWindow, SM_CXSCREEN, SM_CYSCREEN,
    SW_SHOWNOACTIVATE, ULW_ALPHA, WDA_EXCLUDEFROMCAPTURE, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::blur::BlurPatch;

/// Native Win32 UpdateLayeredWindow click-through overlay (0 MB Qt overhead).
pub struct BlurOverlay {
    hwnd: HWND,
    mem_dc: HDC,
    dib: HBITMAP,
    bits_ptr: *mut u8,
    width: usize,
    height: usize,
    had_patches: bool,
}

unsafe impl Send for BlurOverlay {}

impl BlurOverlay {
    pub fn new() -> Result<Self> {
        unsafe {
            let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
            let class_name: Vec<u16> = "CleanScreenGuardOverlayV2\0".encode_utf16().collect();
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinst,
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);

            let width = GetSystemMetrics(SM_CXSCREEN).max(1) as usize;
            let height = GetSystemMetrics(SM_CYSCREEN).max(1) as usize;

            let ex_style = WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOPMOST
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE;

            let hwnd = CreateWindowExW(
                ex_style,
                PCWSTR(class_name.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                width as i32,
                height as i32,
                None,
                None,
                Some(hinst),
                None,
            )?;

            // Exclude overlay from DXGI Desktop Duplication to prevent feedback loops
            let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

            let mem_dc = CreateCompatibleDC(None);
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32), // top-down DIB
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let dib = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
            SelectObject(mem_dc, dib.into());

            Ok(Self {
                hwnd,
                mem_dc,
                dib,
                bits_ptr: bits as *mut u8,
                width,
                height,
                had_patches: false,
            })
        }
    }

    pub fn update_patches(&mut self, patches: &[BlurPatch], debug_mode: bool) {
        if patches.is_empty() && !self.had_patches {
            return;
        }
        unsafe {
            let total = self.width * self.height * 4;
            let dst = std::slice::from_raw_parts_mut(self.bits_ptr, total);
            dst.fill(0);

            for p in patches {
                let x1 = p.x1.clamp(0, self.width as i32) as usize;
                let y1 = p.y1.clamp(0, self.height as i32) as usize;
                let x2 = p.x2.clamp(0, self.width as i32) as usize;
                let y2 = p.y2.clamp(0, self.height as i32) as usize;
                if x2 <= x1 || y2 <= y1 {
                    continue;
                }
                let bw = x2 - x1;
                let bh = y2 - y1;
                let sx = p.width as f32 / bw as f32;
                let sy = p.height as f32 / bh as f32;

                for dy in 0..bh {
                    let fy = (dy as f32 + 0.5) * sy - 0.5;
                    let y0 = (fy.floor() as isize).clamp(0, p.height as isize - 1) as usize;
                    let y1_s = (y0 + 1).min(p.height - 1);
                    let wy = (fy - y0 as f32).clamp(0.0, 1.0);
                    let dst_row = (y1 + dy) * self.width * 4;

                    for dx in 0..bw {
                        let fx = (dx as f32 + 0.5) * sx - 0.5;
                        let x0 = (fx.floor() as isize).clamp(0, p.width as isize - 1) as usize;
                        let x1_s = (x0 + 1).min(p.width - 1);
                        let wx = (fx - x0 as f32).clamp(0.0, 1.0);

                        let i00 = (y0 * p.width + x0) * 4;
                        let i10 = (y0 * p.width + x1_s) * 4;
                        let i01 = (y1_s * p.width + x0) * 4;
                        let i11 = (y1_s * p.width + x1_s) * 4;

                        let d_idx = dst_row + (x1 + dx) * 4;
                        for c in 0..3 {
                            let top = p.bgra[i00 + c] as f32 * (1.0 - wx) + p.bgra[i10 + c] as f32 * wx;
                            let bot = p.bgra[i01 + c] as f32 * (1.0 - wx) + p.bgra[i11 + c] as f32 * wx;
                            dst[d_idx + c] = (top * (1.0 - wy) + bot * wy) as u8;
                        }
                        dst[d_idx + 3] = 255;
                    }
                }

                if debug_mode {
                    // Draw 2px red border around detection box
                    for x in x1..x2 {
                        for border in [y1, (y1 + 1).min(self.height - 1), y2.saturating_sub(1), y2.saturating_sub(2)] {
                            let i = (border * self.width + x) * 4;
                            dst[i] = 68;
                            dst[i + 1] = 68;
                            dst[i + 2] = 239;
                            dst[i + 3] = 255;
                        }
                    }
                    for y in y1..y2 {
                        for border in [x1, (x1 + 1).min(self.width - 1), x2.saturating_sub(1), x2.saturating_sub(2)] {
                            let i = (y * self.width + border) * 4;
                            dst[i] = 68;
                            dst[i + 1] = 68;
                            dst[i + 2] = 239;
                            dst[i + 3] = 255;
                        }
                    }
                }
            }

            let pt_dst = POINT { x: 0, y: 0 };
            let pt_src = POINT { x: 0, y: 0 };
            let sz = SIZE {
                cx: self.width as i32,
                cy: self.height as i32,
            };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };

            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&pt_dst),
                Some(&sz),
                Some(self.mem_dc),
                Some(&pt_src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            self.had_patches = !patches.is_empty();
        }
    }

    pub fn clear(&mut self) {
        self.update_patches(&[], false);
    }
}

impl Drop for BlurOverlay {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.dib.into());
            let _ = DeleteDC(self.mem_dc);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
