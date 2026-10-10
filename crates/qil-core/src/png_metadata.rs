use crc32fast::Hasher;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

pub(crate) struct GenerationMetadata<'a> {
    pub prompt: &'a str,
    pub seed: u64,
    pub steps: u32,
    pub width: u32,
    pub height: u32,
    pub preset: &'a str,
    /// Extra "Key: value" lines appended to the parameters text (e.g. masked-edit details).
    pub extra: Option<&'a str>,
}

pub(crate) fn embed_generation_metadata(
    png: &[u8],
    meta: &GenerationMetadata<'_>,
) -> Result<Vec<u8>, String> {
    if !png.starts_with(PNG_SIGNATURE) {
        return Err("invalid PNG signature".into());
    }

    let mut parameters = format!(
        "{}\n\nModel: Qwen-Image-2.1\nSeed: {}\nSteps: {}\nSize: {}x{}\nPreset: {}",
        meta.prompt, meta.seed, meta.steps, meta.width, meta.height, meta.preset
    );
    if let Some(extra) = meta.extra {
        parameters.push('\n');
        parameters.push_str(extra);
    }
    let xmp = format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description xmlns:Iptc4xmpExt=\"http://iptc.org/std/Iptc4xmpExt/2008-02-29/\" Iptc4xmpExt:AIPromptInformation=\"{}\" Iptc4xmpExt:AISystemUsed=\"PicaLum QwenImage\" Iptc4xmpExt:AISystemVersionUsed=\"Qwen-Image-2.1\" Iptc4xmpExt:DigitalSourceType=\"http://cv.iptc.org/newscodes/digitalsourcetype/trainedAlgorithmicMedia\"/></rdf:RDF></x:xmpmeta>\n<?xpacket end=\"w\"?>",
        escape_xml(meta.prompt)
    );
    let parameters_chunk = itxt_chunk("parameters", &parameters)?;
    let xmp_chunk = itxt_chunk("XML:com.adobe.xmp", &xmp)?;

    let mut output = Vec::with_capacity(png.len() + parameters_chunk.len() + xmp_chunk.len());
    output.extend_from_slice(PNG_SIGNATURE);
    let mut offset = PNG_SIGNATURE.len();
    let mut found_iend = false;
    while offset < png.len() {
        let chunk_start = offset;
        let length_bytes = png
            .get(offset..offset + 4)
            .ok_or_else(|| "truncated PNG chunk length".to_string())?;
        let length = u32::from_be_bytes(length_bytes.try_into().expect("four bytes")) as usize;
        let chunk_end = offset
            .checked_add(12)
            .and_then(|end| end.checked_add(length))
            .filter(|&end| end <= png.len())
            .ok_or_else(|| "truncated PNG chunk".to_string())?;
        let chunk_type = &png[offset + 4..offset + 8];
        if chunk_type == b"IEND" {
            if length != 0 || chunk_end != png.len() {
                return Err("invalid PNG IEND chunk".into());
            }
            output.extend_from_slice(&parameters_chunk);
            output.extend_from_slice(&xmp_chunk);
            output.extend_from_slice(&png[chunk_start..chunk_end]);
            found_iend = true;
            break;
        }
        output.extend_from_slice(&png[chunk_start..chunk_end]);
        offset = chunk_end;
    }
    if !found_iend {
        return Err("PNG is missing IEND".into());
    }
    Ok(output)
}

fn itxt_chunk(keyword: &str, text: &str) -> Result<Vec<u8>, String> {
    let mut data = Vec::with_capacity(keyword.len() + text.len() + 5);
    data.extend_from_slice(keyword.as_bytes());
    data.extend_from_slice(&[0, 0, 0, 0, 0]);
    data.extend_from_slice(text.as_bytes());
    let length = u32::try_from(data.len()).map_err(|_| "PNG metadata chunk is too large")?;
    let mut chunk = Vec::with_capacity(data.len() + 12);
    chunk.extend_from_slice(&length.to_be_bytes());
    chunk.extend_from_slice(b"iTXt");
    chunk.extend_from_slice(&data);
    let mut hasher = Hasher::new();
    hasher.update(b"iTXt");
    hasher.update(&data);
    chunk.extend_from_slice(&hasher.finalize().to_be_bytes());
    Ok(chunk)
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        escaped.push_str(match ch {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&apos;",
            _ => {
                escaped.push(ch);
                continue;
            }
        });
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(data);
        let mut hasher = Hasher::new();
        hasher.update(kind);
        hasher.update(data);
        bytes.extend_from_slice(&hasher.finalize().to_be_bytes());
        bytes
    }

    fn fixture() -> Vec<u8> {
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend(chunk(b"IHDR", &[0; 13]));
        png.extend(chunk(b"tEXt", b"Keep\0original"));
        png.extend(chunk(b"IEND", &[]));
        png
    }

    fn chunks(png: &[u8]) -> Vec<(&[u8], &[u8], &[u8], &[u8])> {
        let mut offset = PNG_SIGNATURE.len();
        let mut chunks = Vec::new();
        while offset < png.len() {
            let length = u32::from_be_bytes(png[offset..offset + 4].try_into().unwrap()) as usize;
            chunks.push((
                &png[offset..offset + 4],
                &png[offset + 4..offset + 8],
                &png[offset + 8..offset + 8 + length],
                &png[offset + 8 + length..offset + 12 + length],
            ));
            offset += length + 12;
        }
        chunks
    }

    #[test]
    fn embeds_utf8_parameters_and_escaped_xmp_without_changing_existing_chunks() {
        let original = fixture();
        let meta = GenerationMetadata {
            prompt: "кот & <луна>",
            seed: 42,
            steps: 30,
            width: 1024,
            height: 768,
            preset: "standard",
            extra: None,
        };
        let embedded = embed_generation_metadata(&original, &meta).unwrap();
        let old_chunks = chunks(&original);
        let new_chunks = chunks(&embedded);
        assert_eq!(new_chunks.len(), old_chunks.len() + 2);
        assert_eq!(old_chunks[0], new_chunks[0]);
        assert_eq!(old_chunks[1], new_chunks[1]);
        assert_eq!(old_chunks[2], new_chunks[4]);
        assert_eq!(new_chunks[2].1, b"iTXt");
        assert_eq!(new_chunks[3].1, b"iTXt");
        assert_eq!(new_chunks[4].1, b"IEND");
        let mut payloads = Vec::new();
        for chunk in &new_chunks[2..4] {
            let data = chunk.2;
            let keyword_end = data.iter().position(|&byte| byte == 0).unwrap();
            let text = &data[keyword_end + 5..];
            let mut hasher = Hasher::new();
            hasher.update(chunk.1);
            hasher.update(data);
            assert_eq!(hasher.finalize().to_be_bytes().as_slice(), chunk.3);
            payloads.push((
                std::str::from_utf8(&data[..keyword_end]).unwrap(),
                std::str::from_utf8(text).unwrap(),
            ));
        }
        assert_eq!(payloads[0].0, "parameters");
        assert!(payloads[0].1.contains("кот & <луна>"));
        assert!(payloads[0].1.contains(
            "Model: Qwen-Image-2.1\nSeed: 42\nSteps: 30\nSize: 1024x768\nPreset: standard"
        ));
        assert_eq!(payloads[1].0, "XML:com.adobe.xmp");
        assert!(payloads[1].1.contains("кот &amp; &lt;луна&gt;"));
        assert!(payloads[1].1.contains("PicaLum QwenImage"));
    }

    #[test]
    fn rejects_truncated_png_and_missing_iend() {
        assert!(embed_generation_metadata(
            b"not png",
            &GenerationMetadata {
                prompt: "",
                seed: 0,
                steps: 1,
                width: 1,
                height: 1,
                preset: "draft",
                extra: None
            }
        )
        .is_err());
        assert!(embed_generation_metadata(
            &PNG_SIGNATURE[..],
            &GenerationMetadata {
                prompt: "",
                seed: 0,
                steps: 1,
                width: 1,
                height: 1,
                preset: "draft",
                extra: None
            }
        )
        .is_err());
    }
}
