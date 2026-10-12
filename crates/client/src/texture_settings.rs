//! The Graphics page's texture settings as retail's renderer applies them:
//! `r_picmip`, `r_picmip2` and `r_texturebits` at image load (latched, so a
//! change waits for `vid_restart`) and `r_textureMode` on the samplers
//! (docs/research/cod11-front-end.md, section 14, "Texture settings").

use vcod_common::assets::{Image, ImageData};

/// What one renderer build loads and samples images with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureSettings {
    /// `r_picmip`: mip levels dropped from world images, 0..=3.
    pub picmip: u32,
    /// `r_picmip2`: the same for model images (Character Textures).
    pub picmip2: u32,
    /// `r_texturebits`: 16 stores uncompressed images at 16 bits a texel.
    pub bits: u32,
    pub filter: TextureFilter,
}

impl Default for TextureSettings {
    /// vcod's cvar defaults: every level kept, `r_texturebits` 0,
    /// `GL_LINEAR_MIPMAP_NEAREST`.
    fn default() -> Self {
        TextureSettings {
            picmip: 0,
            picmip2: 0,
            bits: 0,
            filter: TextureFilter::by_name("GL_LINEAR_MIPMAP_NEAREST").unwrap(),
        }
    }
}

impl TextureSettings {
    /// The settings `r_picmip`, `r_picmip2`, `r_texturebits` and
    /// `r_textureMode` read as; a filter name retail would refuse keeps
    /// `prev`'s filter, as `GL_TextureMode` leaves the images alone.
    pub fn from_cvars(
        picmip: f32,
        picmip2: f32,
        bits: f32,
        texture_mode: &str,
        prev: TextureFilter,
    ) -> TextureSettings {
        TextureSettings {
            picmip: picmip_level(picmip),
            picmip2: picmip_level(picmip2),
            bits: bits as i32 as u32,
            filter: TextureFilter::by_name(texture_mode).unwrap_or(prev),
        }
    }

    /// Levels to drop from an image of a world shader (`nopicmip` keeps
    /// every level).
    pub fn world_picmip(&self, nopicmip: bool) -> u32 {
        if nopicmip { 0 } else { self.picmip }
    }

    /// Levels to drop from a model's image: a shader registered without a
    /// lightmap starts with the `picmip2` flag (0x4fc6f3).
    pub fn model_picmip(&self, nopicmip: bool) -> u32 {
        if nopicmip { 0 } else { self.picmip2 }
    }
}

/// `0x4ea380`: the cvar's integer clamped to 0..=3.
fn picmip_level(v: f32) -> u32 {
    (v as i32).clamp(0, 3) as u32
}

/// One row of `GL_TextureMode`'s table (CoDMP.exe 0x5712f8): the min and
/// mag filters of a mipmapped image. `mip` `None` is a min filter without
/// mipmaps, which samples level 0 only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureFilter {
    pub min: wgpu::FilterMode,
    pub mag: wgpu::FilterMode,
    pub mip: Option<wgpu::MipmapFilterMode>,
}

impl TextureFilter {
    /// The six names `GL_TextureMode` (0x4ea530) accepts, compared without
    /// case; anything else prints `bad filter name`.
    pub fn by_name(name: &str) -> Option<TextureFilter> {
        use wgpu::FilterMode::{Linear, Nearest};
        use wgpu::MipmapFilterMode as M;
        let (min, mag, mip) = match name.to_ascii_uppercase().as_str() {
            "GL_NEAREST" => (Nearest, Nearest, None),
            "GL_LINEAR" => (Linear, Linear, None),
            "GL_NEAREST_MIPMAP_NEAREST" => (Nearest, Nearest, Some(M::Nearest)),
            "GL_LINEAR_MIPMAP_NEAREST" => (Linear, Linear, Some(M::Nearest)),
            "GL_NEAREST_MIPMAP_LINEAR" => (Nearest, Nearest, Some(M::Linear)),
            "GL_LINEAR_MIPMAP_LINEAR" => (Linear, Linear, Some(M::Linear)),
            _ => return None,
        };
        Some(TextureFilter { min, mag, mip })
    }

    /// A sampler descriptor for mipmapped images under this filter. vcod's
    /// 16x anisotropy rides only the trilinear mode, the one wgpu allows it
    /// with; retail 1.1 forces `r_ext_texture_filter_anisotropic` to 0.
    pub fn sampler<'a>(
        &self,
        label: &'a str,
        address: wgpu::AddressMode,
    ) -> wgpu::SamplerDescriptor<'a> {
        let trilinear = self.min == wgpu::FilterMode::Linear
            && self.mag == wgpu::FilterMode::Linear
            && self.mip == Some(wgpu::MipmapFilterMode::Linear);
        wgpu::SamplerDescriptor {
            label: Some(label),
            address_mode_u: address,
            address_mode_v: address,
            address_mode_w: address,
            mag_filter: self.mag,
            min_filter: self.min,
            mipmap_filter: self.mip.unwrap_or(wgpu::MipmapFilterMode::Nearest),
            lod_max_clamp: if self.mip.is_some() { 32.0 } else { 0.0 },
            anisotropy_clamp: if trilinear { 16 } else { 1 },
            ..Default::default()
        }
    }
}

/// An image as retail uploads it: `picmip` levels dropped, the texture
/// gamma baked in (`gamma::bake_image`), then `r_texturebits`.
/// Block-compressed images lose whole mip levels while both sides stay
/// above 4 (0x4ef590); uncompressed ones are halved to `size >> picmip`
/// (0x4ebf20) and stored at 16 bits a texel with `r_texturebits 16`
/// (0x4ebe30).
pub fn prepare(img: &mut Image, picmip: u32, gamma: Option<&[u8; 256]>, bits: u32) {
    drop_levels(img, picmip);
    crate::gamma::bake_image(img, gamma);
    if bits == 16
        && let ImageData::Rgba8(px) = &mut img.data
    {
        quantize_16(px);
    }
}

fn drop_levels(img: &mut Image, picmip: u32) {
    match &mut img.data {
        ImageData::Bc { mips, .. } => {
            let mut drop = 0;
            let (mut w, mut h) = (img.width, img.height);
            while drop < picmip && w >= 5 && h >= 5 && mips.len() > drop as usize + 1 {
                w >>= 1;
                h >>= 1;
                drop += 1;
            }
            mips.drain(..drop as usize);
            img.width = w;
            img.height = h;
        }
        ImageData::Rgba8(px) => {
            for _ in 0..picmip {
                if img.width == 1 && img.height == 1 {
                    break;
                }
                let (w, h, half) = halve(img.width, img.height, px);
                *px = half;
                img.width = w;
                img.height = h;
            }
        }
    }
}

/// One 2x2 box step down (a side of 1 stays 1).
fn halve(w: u32, h: u32, px: &[u8]) -> (u32, u32, Vec<u8>) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let xs = [2 * x, (2 * x + 1).min(w - 1)];
            let ys = [2 * y, (2 * y + 1).min(h - 1)];
            for c in 0..4 {
                let mut sum = 0u32;
                for &sy in &ys {
                    for &sx in &xs {
                        sum += u32::from(px[((sy * w + sx) * 4 + c) as usize]);
                    }
                }
                out[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
            }
        }
    }
    (nw, nh, out)
}

/// `r_texturebits 16`: an image with any alpha below 255 goes up as
/// `GL_RGBA4`, an opaque one as `GL_RGB5` (0x4ebe30). The driver's rounding
/// is not in the binary; this rounds to nearest and widens back.
fn quantize_16(px: &mut [u8]) {
    let opaque = px.iter().skip(3).step_by(4).all(|&a| a == 255);
    let q = |v: u8, bits: u32| -> u8 {
        let max = (1u32 << bits) - 1;
        let n = (u32::from(v) * max + 127) / 255;
        ((n * 255 + max / 2) / max) as u8
    };
    for t in px.as_chunks_mut::<4>().0 {
        if opaque {
            for c in &mut t[..3] {
                *c = q(*c, 5);
            }
        } else {
            for c in t.iter_mut() {
                *c = q(*c, 4);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcod_common::assets::TextureFormat;

    fn rgba(w: u32, h: u32, px: Vec<u8>) -> Image {
        Image {
            width: w,
            height: h,
            data: ImageData::Rgba8(px),
        }
    }

    #[test]
    fn picmip_cvars_clamp_to_three_levels() {
        let f = TextureFilter::by_name("GL_LINEAR").unwrap();
        let s = TextureSettings::from_cvars(-1.0, 7.0, 16.0, "gl_linear", f);
        assert_eq!((s.picmip, s.picmip2, s.bits), (0, 3, 16));
        assert_eq!(s.world_picmip(false), 0);
        assert_eq!(s.model_picmip(false), 3);
        assert_eq!(s.model_picmip(true), 0);
    }

    #[test]
    fn texture_modes_map_to_retail_filters() {
        use wgpu::FilterMode::{Linear, Nearest};
        use wgpu::MipmapFilterMode as M;
        let bilinear = TextureFilter::by_name("GL_LINEAR_MIPMAP_NEAREST").unwrap();
        assert_eq!(
            (bilinear.min, bilinear.mag, bilinear.mip),
            (Linear, Linear, Some(M::Nearest))
        );
        let tri = TextureFilter::by_name("gl_linear_mipmap_linear").unwrap();
        assert_eq!(tri.mip, Some(M::Linear));
        let near = TextureFilter::by_name("GL_NEAREST").unwrap();
        assert_eq!((near.min, near.mip), (Nearest, None));
        assert_eq!(
            near.sampler("t", wgpu::AddressMode::Repeat).lod_max_clamp,
            0.0
        );
        assert_eq!(
            tri.sampler("t", wgpu::AddressMode::Repeat).anisotropy_clamp,
            16
        );
        assert_eq!(
            bilinear
                .sampler("t", wgpu::AddressMode::Repeat)
                .anisotropy_clamp,
            1
        );
        // a refused name keeps the filter already set
        let s = TextureSettings::from_cvars(1.0, 2.0, 0.0, "GL_CUBIC", tri);
        assert_eq!(s.filter, tri);
    }

    #[test]
    fn picmip_halves_uncompressed_images() {
        let mut img = rgba(4, 2, (0..32).map(|i| (i * 8) as u8).collect());
        prepare(&mut img, 1, None, 0);
        assert_eq!((img.width, img.height), (2, 1));
        let ImageData::Rgba8(px) = &img.data else {
            unreachable!()
        };
        // red of texels 0, 1, 4 and 5 (0, 32, 128, 160) averaged
        assert_eq!(px[0], 80);
        prepare(&mut img, 3, None, 0);
        assert_eq!((img.width, img.height), (1, 1));
    }

    #[test]
    fn picmip_drops_compressed_levels_while_sides_exceed_four() {
        let mips = (0..4).map(|l| vec![l as u8; 8]).collect();
        let mut img = Image {
            width: 16,
            height: 8,
            data: ImageData::Bc {
                format: TextureFormat::Bc1RgbaUnorm,
                mips,
            },
        };
        prepare(&mut img, 3, None, 16);
        // 16x8 -> 8x4, then 4 is not above 4
        assert_eq!((img.width, img.height), (8, 4));
        let ImageData::Bc { mips, .. } = &img.data else {
            unreachable!()
        };
        assert_eq!(mips.len(), 3);
        assert_eq!(mips[0][0], 1);
    }

    #[test]
    fn sixteen_bits_quantize_by_alpha() {
        let mut opaque = rgba(1, 1, vec![100, 200, 7, 255]);
        prepare(&mut opaque, 0, None, 16);
        let ImageData::Rgba8(px) = &opaque.data else {
            unreachable!()
        };
        // 5 bits: 100 -> 12 -> 99, 200 -> 24 -> 197, 7 -> 1 -> 8
        assert_eq!(px.as_slice(), &[99, 197, 8, 255]);
        let mut alpha = rgba(1, 1, vec![100, 200, 7, 128]);
        prepare(&mut alpha, 0, None, 16);
        let ImageData::Rgba8(px) = &alpha.data else {
            unreachable!()
        };
        // 4 bits: 100 -> 6 -> 102, 200 -> 12 -> 204, 7 -> 0, 128 -> 8 -> 136
        assert_eq!(px.as_slice(), &[102, 204, 0, 136]);
        let mut kept = rgba(1, 1, vec![100, 200, 7, 128]);
        prepare(&mut kept, 0, None, 32);
        let ImageData::Rgba8(px) = &kept.data else {
            unreachable!()
        };
        assert_eq!(px.as_slice(), &[100, 200, 7, 128]);
    }
}
