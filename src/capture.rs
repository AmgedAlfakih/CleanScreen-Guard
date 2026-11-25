use anyhow::{anyhow, Result};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView,
    ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_RESOURCE_MISC_GENERATE_MIPS, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
    DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    /// GPU-downscaled mip (~480x270 on 1080p = 0.5 MB) for motion detection + 320n scout
    Scout,
    /// GPU-downscaled mip (~960x540 on 1080p = 2.0 MB) for 640m verification
    Verify,
    /// Full native resolution for crisp blurring + GPU-downscaled AI frame
    FullAndAi,
}

pub struct CapturedFrame {
    pub screen_width: usize,
    pub screen_height: usize,
    pub ai_width: usize,
    pub ai_height: usize,
    pub ai_bgra: Vec<u8>,
    pub full_bgra: Option<Vec<u8>>,
}

struct GpuMipScaler {
    mip_tex: ID3D11Texture2D,
    mip_srv: ID3D11ShaderResourceView,
    ai_mip: u32,
    ai_w: usize,
    ai_h: usize,
    ai_staging: ID3D11Texture2D,
    scout_mip: u32,
    scout_w: usize,
    scout_h: usize,
    scout_staging: ID3D11Texture2D,
}

/// Native Windows DXGI Desktop Duplication capturer with D3D11 GPU-side mipmap downscaling.
pub struct DesktopCapturer {
    monitor_index: u32,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    full_staging: Option<ID3D11Texture2D>,
    mip_scaler: Option<GpuMipScaler>,
    mip_scaler_failed: bool,
    has_captured_once: bool,
    width: usize,
    height: usize,
}

fn choose_mip(w: usize, h: usize, min_long_edge: usize) -> (u32, usize, usize) {
    let mut mip = 0u32;
    while mip < 6 {
        let nw = (w >> (mip + 1)).max(1);
        let nh = (h >> (mip + 1)).max(1);
        if nw.max(nh) >= min_long_edge {
            mip += 1;
        } else {
            break;
        }
    }
    (mip, (w >> mip).max(1), (h >> mip).max(1))
}

impl DesktopCapturer {
    pub fn new(monitor_index: u32) -> Result<Self> {
        let (device, context, duplication, width, height) = Self::init_dxgi(monitor_index)?;
        Ok(Self {
            monitor_index,
            device,
            context,
            duplication,
            full_staging: None,
            mip_scaler: None,
            mip_scaler_failed: false,
            has_captured_once: false,
            width,
            height,
        })
    }

    fn init_dxgi(
        monitor_index: u32,
    ) -> Result<(ID3D11Device, ID3D11DeviceContext, IDXGIOutputDuplication, usize, usize)> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
            let mut global_idx = 0u32;
            let mut adapter_idx = 0u32;

            while let Ok(adapter) = factory.EnumAdapters1(adapter_idx) {
                let mut out_idx = 0u32;
                while let Ok(output) = adapter.EnumOutputs(out_idx) {
                    if global_idx == monitor_index {
                        let desc = output.GetDesc()?;
                        let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left).max(1) as usize;
                        let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top).max(1) as usize;

                        let mut device: Option<ID3D11Device> = None;
                        let mut context: Option<ID3D11DeviceContext> = None;
                        D3D11CreateDevice(
                            &adapter,
                            D3D_DRIVER_TYPE_UNKNOWN,
                            windows::Win32::Foundation::HMODULE::default(),
                            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                            None,
                            D3D11_SDK_VERSION,
                            Some(&mut device),
                            None,
                            Some(&mut context),
                        )?;

                        let device = device.ok_or_else(|| anyhow!("D3D11 device creation returned None"))?;
                        let context = context.ok_or_else(|| anyhow!("D3D11 context creation returned None"))?;
                        let output1: IDXGIOutput1 = output.cast()?;
                        let duplication = output1.DuplicateOutput(&device)?;
                        return Ok((device, context, duplication, width, height));
                    }
                    global_idx += 1;
                    out_idx += 1;
                }
                adapter_idx += 1;
            }
            Err(anyhow!("Monitor index {monitor_index} not found"))
        }
    }

    unsafe fn create_staging(&self, base_desc: &D3D11_TEXTURE2D_DESC, w: usize, h: usize) -> Option<ID3D11Texture2D> {
        let mut sdesc = *base_desc;
        sdesc.Width = w as u32;
        sdesc.Height = h as u32;
        sdesc.MipLevels = 1;
        sdesc.ArraySize = 1;
        sdesc.SampleDesc.Count = 1;
        sdesc.SampleDesc.Quality = 0;
        sdesc.Usage = D3D11_USAGE_STAGING;
        sdesc.BindFlags = 0;
        sdesc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        sdesc.MiscFlags = 0;
        let mut stg: Option<ID3D11Texture2D> = None;
        unsafe {
            self.device.CreateTexture2D(&sdesc, None, Some(&mut stg)).ok()?;
        }
        stg
    }

    unsafe fn init_mip_scaler(&self, desc: &D3D11_TEXTURE2D_DESC, w: usize, h: usize) -> Option<GpuMipScaler> {
        let (ai_mip, ai_w, ai_h) = choose_mip(w, h, 640);
        let (scout_mip, scout_w, scout_h) = choose_mip(w, h, 320);

        let mut mdesc = *desc;
        mdesc.MipLevels = 0; // Full hardware mipmap chain
        mdesc.ArraySize = 1;
        mdesc.SampleDesc.Count = 1;
        mdesc.SampleDesc.Quality = 0;
        mdesc.Usage = D3D11_USAGE_DEFAULT;
        mdesc.BindFlags = (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32;
        mdesc.CPUAccessFlags = 0;
        mdesc.MiscFlags = D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32;

        let (mip_tex, mip_srv, ai_staging, scout_staging) = unsafe {
            let mut mip_tex: Option<ID3D11Texture2D> = None;
            self.device.CreateTexture2D(&mdesc, None, Some(&mut mip_tex)).ok()?;
            let mip_tex = mip_tex?;

            let mut mip_srv: Option<ID3D11ShaderResourceView> = None;
            self.device.CreateShaderResourceView(&mip_tex, None, Some(&mut mip_srv)).ok()?;
            let mip_srv = mip_srv?;

            let ai_staging = self.create_staging(desc, ai_w, ai_h)?;
            let scout_staging = self.create_staging(desc, scout_w, scout_h)?;
            (mip_tex, mip_srv, ai_staging, scout_staging)
        };

        Some(GpuMipScaler {
            mip_tex,
            mip_srv,
            ai_mip,
            ai_w,
            ai_h,
            ai_staging,
            scout_mip,
            scout_w,
            scout_h,
            scout_staging,
        })
    }

    unsafe fn map_staging(&self, staging: &ID3D11Texture2D, w: usize, h: usize) -> Option<Vec<u8>> {
        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .ok()?;

            let row_bytes = w * 4;
            let pitch = mapped.RowPitch as usize;
            let mut bgra = vec![0u8; row_bytes * h];
            let src_base = mapped.pData as *const u8;
            for y in 0..h {
                let src = std::slice::from_raw_parts(src_base.add(y * pitch), row_bytes);
                bgra[y * row_bytes..(y + 1) * row_bytes].copy_from_slice(src);
            }

            self.context.Unmap(staging, 0);
            Some(bgra)
        }
    }

    /// Reads back the latest full-resolution desktop frame stored in VRAM (mip 0)
    /// on demand when detections appear while running in low-bandwidth Scout/Verify mode.
    pub fn readback_full_res(&mut self) -> Option<(Vec<u8>, usize, usize)> {
        if !self.has_captured_once {
            return None;
        }
        unsafe {
            let full_staging = self.full_staging.as_ref()?;
            if let Some(ref scaler) = self.mip_scaler {
                self.context
                    .CopySubresourceRegion(full_staging, 0, 0, 0, 0, &scaler.mip_tex, 0, None);
            }
            let bgra = self.map_staging(full_staging, self.width, self.height)?;
            Some((bgra, self.width, self.height))
        }
    }

    pub fn grab(&mut self, timeout_ms: u32, mode: CaptureMode) -> Option<CapturedFrame> {
        unsafe {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource: Option<IDXGIResource> = None;

            match self.duplication.AcquireNextFrame(timeout_ms, &mut info, &mut resource) {
                Ok(()) => {}
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return None,
                Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => {
                    if let Ok((d, c, dup, w, h)) = Self::init_dxgi(self.monitor_index) {
                        self.device = d;
                        self.context = c;
                        self.duplication = dup;
                        self.full_staging = None;
                        self.mip_scaler = None;
                        self.mip_scaler_failed = false;
                        self.has_captured_once = false;
                        self.width = w;
                        self.height = h;
                    }
                    return None;
                }
                Err(_) => return None,
            }

            // Skip cursor-only updates (zero desktop pixel changes -> 0 GPU copy & 0 PCIe transfer)
            if self.has_captured_once && info.LastPresentTime == 0 {
                let _ = self.duplication.ReleaseFrame();
                return None;
            }

            let result = (|| -> Option<CapturedFrame> {
                let res = resource?;
                let tex: ID3D11Texture2D = res.cast().ok()?;
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                tex.GetDesc(&mut desc);
                let w = desc.Width as usize;
                let h = desc.Height as usize;

                if self.full_staging.is_none() || self.width != w || self.height != h {
                    self.full_staging = self.create_staging(&desc, w, h);
                    self.mip_scaler = None;
                    self.mip_scaler_failed = false;
                    self.width = w;
                    self.height = h;
                }

                if self.mip_scaler.is_none() && !self.mip_scaler_failed {
                    match self.init_mip_scaler(&desc, w, h) {
                        Some(s) => self.mip_scaler = Some(s),
                        None => self.mip_scaler_failed = true,
                    }
                }

                if let Some(ref scaler) = self.mip_scaler {
                    // Copy desktop frame into mip 0 and generate downscaled mips entirely on the GPU
                    self.context
                        .CopySubresourceRegion(&scaler.mip_tex, 0, 0, 0, 0, &tex, 0, None);
                    self.context.GenerateMips(&scaler.mip_srv);
                    self.has_captured_once = true;

                    let (ai_bgra, ai_w, ai_h) = match mode {
                        CaptureMode::Scout => {
                            self.context.CopySubresourceRegion(
                                &scaler.scout_staging,
                                0,
                                0,
                                0,
                                0,
                                &scaler.mip_tex,
                                scaler.scout_mip,
                                None,
                            );
                            (
                                self.map_staging(&scaler.scout_staging, scaler.scout_w, scaler.scout_h)?,
                                scaler.scout_w,
                                scaler.scout_h,
                            )
                        }
                        CaptureMode::Verify | CaptureMode::FullAndAi => {
                            self.context.CopySubresourceRegion(
                                &scaler.ai_staging,
                                0,
                                0,
                                0,
                                0,
                                &scaler.mip_tex,
                                scaler.ai_mip,
                                None,
                            );
                            (
                                self.map_staging(&scaler.ai_staging, scaler.ai_w, scaler.ai_h)?,
                                scaler.ai_w,
                                scaler.ai_h,
                            )
                        }
                    };

                    let full_bgra = if mode == CaptureMode::FullAndAi {
                        let full_staging = self.full_staging.as_ref()?;
                        self.context.CopyResource(full_staging, &tex);
                        Some(self.map_staging(full_staging, w, h)?)
                    } else {
                        None
                    };

                    Some(CapturedFrame {
                        screen_width: w,
                        screen_height: h,
                        ai_width: ai_w,
                        ai_height: ai_h,
                        ai_bgra,
                        full_bgra,
                    })
                } else {
                    // Safe fallback if driver/format refuses hardware mipgen
                    let full_staging = self.full_staging.as_ref()?;
                    self.context.CopyResource(full_staging, &tex);
                    self.has_captured_once = true;
                    let bgra = self.map_staging(full_staging, w, h)?;
                    let full_bgra = if mode == CaptureMode::FullAndAi {
                        Some(bgra.clone())
                    } else {
                        None
                    };
                    Some(CapturedFrame {
                        screen_width: w,
                        screen_height: h,
                        ai_width: w,
                        ai_height: h,
                        ai_bgra: bgra,
                        full_bgra,
                    })
                }
            })();

            let _ = self.duplication.ReleaseFrame();
            result
        }
    }
}
