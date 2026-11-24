use anyhow::{anyhow, Result};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
    DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
};

pub struct CapturedFrame {
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
}

/// Native Windows DXGI Desktop Duplication capturer (raw BGRA, zero Python/dxcam overhead).
pub struct DesktopCapturer {
    monitor_index: u32,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging: Option<ID3D11Texture2D>,
    width: usize,
    height: usize,
}

impl DesktopCapturer {
    pub fn new(monitor_index: u32) -> Result<Self> {
        let (device, context, duplication, width, height) = Self::init_dxgi(monitor_index)?;
        Ok(Self {
            monitor_index,
            device,
            context,
            duplication,
            staging: None,
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

    pub fn grab(&mut self, timeout_ms: u32) -> Option<CapturedFrame> {
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
                        self.staging = None;
                        self.width = w;
                        self.height = h;
                    }
                    return None;
                }
                Err(_) => return None,
            }

            let result = (|| -> Option<CapturedFrame> {
                let res = resource?;
                let tex: ID3D11Texture2D = res.cast().ok()?;
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                tex.GetDesc(&mut desc);
                let w = desc.Width as usize;
                let h = desc.Height as usize;

                if self.staging.is_none() || self.width != w || self.height != h {
                    let mut sdesc = desc;
                    sdesc.Usage = D3D11_USAGE_STAGING;
                    sdesc.BindFlags = 0;
                    sdesc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                    sdesc.MiscFlags = 0;
                    let mut stg: Option<ID3D11Texture2D> = None;
                    self.device.CreateTexture2D(&sdesc, None, Some(&mut stg)).ok()?;
                    self.staging = stg;
                    self.width = w;
                    self.height = h;
                }

                let staging = self.staging.as_ref()?;
                self.context.CopyResource(staging, &tex);

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
                Some(CapturedFrame { width: w, height: h, bgra })
            })();

            let _ = self.duplication.ReleaseFrame();
            result
        }
    }
}
