// A PDF written by hand: one catalog, one page tree, one content stream. The
// text is drawn with the base-14 font so no font table is needed.
use super::zip::wrap_lines;

pub(super) fn esc_pdf(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            _ if c.is_control() => {}
            _ => out.push(c),
        }
    }
    out
}

pub(super) fn pdf_bytes(title: &str, body: &str) -> Vec<u8> {
    let mut text_lines: Vec<String> = vec![];
    if !title.trim().is_empty() {
        text_lines.push(title.trim().to_string());
        text_lines.push(String::new());
    }
    text_lines.extend(wrap_lines(body, 88));
    let per_page: usize = 44;
    let pages: Vec<Vec<String>> = text_lines.chunks(per_page).map(|c| c.to_vec()).collect();
    let npages = pages.len().max(1);
    let mut objects: Vec<Vec<u8>> = vec![];
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    // Pages start at object 5: 1 catalog, 2 this tree, 3 and 4 the two fonts.
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            (0..npages)
                .map(|i| format!("{} 0 R", 5 + i * 2))
                .collect::<Vec<_>>()
                .join(" "),
            npages
        )
        .into_bytes(),
    );
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec());
    objects.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>".to_vec());
    for (i, lines) in pages.iter().enumerate() {
        let content = lines
            .iter()
            .enumerate()
            .map(|(j, l)| format!("BT /F1 11 Tf 56 {} Td ({}) Tj ET", 760 - j * 16, esc_pdf(l)))
            .collect::<Vec<_>>()
            .join("\n");
        let page_obj = (5 + i * 2) as u32;
        let stream_obj = page_obj + 1;
        objects.push(
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>", stream_obj).into_bytes(),
        );
        let mut stream = vec![];
        stream.extend_from_slice(format!("<< /Length {} >>\nstream\n", content.len()).as_bytes());
        stream.extend_from_slice(content.as_bytes());
        stream.extend_from_slice(b"\nendstream");
        objects.push(stream);
    }
    let mut out: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets: Vec<usize> = vec![];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF",
            objects.len() + 1,
            xref
        )
        .as_bytes(),
    );
    out
}
