use std::io::{BufWriter, Read, Write};

use psip::imgur::Post;

fn main() {
    let mut args = std::env::args_os();
    let _name = args.next();

    let mut buf = Vec::new();

    let mut samples = Vec::new();
    let mut sample_sizes = Vec::new();

    let mut spells = Vec::new();

    for p in args {
        let Ok(mut file) = std::fs::OpenOptions::new().read(true).open(&p) else {
            eprintln!("{p:?}: Error opening file");
            continue;
        };

        buf.clear();
        if let Err(e) = file.read_to_end(&mut buf) {
            eprintln!("{p:?}: Error reading file: {e:?}");
            continue;
        }

        let Ok(s) = std::str::from_utf8(&buf) else {
            eprintln!("{p:?}: Spell is not UTF-8");
            continue;
        };

        let Ok(mut post) = serde_json::from_str::<Post>(s) else {
            eprintln!("{p:?}: Post is malformed");
            continue;
        };

        let mut process = |id: String, mut desc: &str| {
            let id = &*id.leak();
            while let Some(brace) = desc.as_bytes().iter().position(|&b| b == b'{') {
                desc = &desc[brace..];
                let end = desc
                    .as_bytes()
                    .iter()
                    .position(|&b| b == b'\n')
                    .unwrap_or(desc.len());

                let s = &desc[..end];
                desc = &desc[end..];

                let mut spell = match psi_spell_encode::snbt_to_spell(s) {
                    Ok(spell) => spell,
                    Err(e) => {
                        eprintln!("{id}: Spell is malformed: {e:?}");
                        continue;
                    }
                };

                let mut comments = Vec::new();
                for (idx, piece) in spell.pieces.iter_mut().enumerate() {
                    if let Some(comment) = std::mem::take(&mut piece.data.comment) {
                        comments.push((idx, comment));
                    }
                }

                let name = std::mem::take(&mut spell.name);

                let len_before = samples.len();
                spell.extend_bin(&mut samples);
                let bin_size = samples.len() - len_before;
                sample_sizes.push(bin_size);

                for (idx, comment) in comments {
                    spell.pieces[idx].data.comment = Some(comment)
                }
                spell.name = name;

                let len_before = samples.len();
                spell.extend_bin(&mut samples);
                let bin_size = samples.len() - len_before;
                samples.truncate(len_before);

                spells.push((id, s.len(), bin_size, spell));
            }
        };

        if let Some(s) = post.data.description {
            eprintln!("{}: Processing post description", &post.data.id);
            process(std::mem::take(&mut post.data.id), &s);
        }

        for mut image in post.data.images {
            if let Some(desc) = image.description {
                eprintln!("{}: Processing image description", &image.id);
                process(std::mem::take(&mut image.id), &desc);
            }
        }
    }

    if sample_sizes.is_empty() {
        return;
    }

    match zstd::dict::from_continuous(&samples, &sample_sizes, 5 << 20) {
        Ok(dict) => {
            let mut buf = Vec::new();
            let mut encoder = zstd::Encoder::with_dictionary(&mut buf, 22, &dict).unwrap();

            let mut spell_buf = Vec::new();
            for (id, snbt_len, bin_len, spell) in spells {
                encoder.get_mut().clear();
                spell_buf.clear();
                spell.extend_bin(&mut spell_buf);
                encoder.write_all(&spell_buf).unwrap();
                encoder.flush().unwrap();
                let dict_len = encoder.get_ref().len();
                let base64_len = base64::encoded_len(dict_len, false).unwrap();
                eprintln!(
                    "{id}: {} -> {} (-{:.2}%) -> {} (-{:.2}%) -> {} (-{:.2}%)",
                    snbt_len,
                    bin_len,
                    100f64 - (bin_len as f64) / (snbt_len as f64) * 100f64,
                    dict_len,
                    100f64 - (dict_len as f64) / (snbt_len as f64) * 100f64,
                    base64_len,
                    100f64 - (base64_len as f64) / (snbt_len as f64) * 100f64,
                );
            }

            if let Err(e) = BufWriter::new(std::io::stdout().lock()).write_all(&dict) {
                eprintln!("Unable to write dictionary to stdout: {e:?}");
            }
        }
        Err(e) => eprintln!("Unable to train dictionary: {e:?}"),
    }
}
