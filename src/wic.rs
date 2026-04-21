use windows::{
    core::GUID,
    Win32::Graphics::Imaging::*,
    Win32::System::Com::*,
};

pub fn decode_rgba(bytes: &[u8]) -> Result<::image::RgbaImage, String> {
    unsafe {
        // スレッドごとにCOM初期化（既初期化はS_FALSEで無視）
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let factory: IWICImagingFactory = CoCreateInstance(
            &CLSID_WICImagingFactory,
            None,
            CLSCTX_INPROC_SERVER,
        ).map_err(|e| format!("WIC factory: {e}"))?;

        let stream = factory.CreateStream()
            .map_err(|e| format!("WIC stream: {e}"))?;
        stream.InitializeFromMemory(bytes)
            .map_err(|e| format!("WIC stream init: {e}"))?;

        let decoder = factory.CreateDecoderFromStream(
            &stream,
            std::ptr::null(),
            WICDecodeMetadataCacheOnDemand,
        ).map_err(|e| format!("WIC decoder: {e}"))?;

        let frame = decoder.GetFrame(0)
            .map_err(|e| format!("WIC frame: {e}"))?;

        let mut width = 0u32;
        let mut height = 0u32;
        frame.GetSize(&mut width, &mut height)
            .map_err(|e| format!("WIC size: {e}"))?;

        let converter = factory.CreateFormatConverter()
            .map_err(|e| format!("WIC converter: {e}"))?;

        // GUID_WICPixelFormat32bppRGBA
        let rgba_fmt = GUID {
            data1: 0xf5c7ad2d,
            data2: 0x6a8d,
            data3: 0x43dd,
            data4: [0xa7, 0xa8, 0xa2, 0x99, 0x35, 0x26, 0x1a, 0xe9],
        };

        converter.Initialize(
            &frame,
            &rgba_fmt,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeMedianCut,
        ).map_err(|e| format!("WIC convert: {e}"))?;

        let stride = width * 4;
        let mut buffer = vec![0u8; (stride * height) as usize];
        converter.CopyPixels(std::ptr::null(), stride, &mut buffer)
            .map_err(|e| format!("WIC copy: {e}"))?;

        ::image::RgbaImage::from_raw(width, height, buffer)
            .ok_or_else(|| "WIC: buffer size mismatch".to_string())
    }
}
