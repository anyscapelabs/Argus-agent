// PPTX writing: a slide layout per slide, plus the presentation part that ties
// them together.
use super::zip::{esc_xml, ZipWriter};

/// One text box on a slide. The loose geometry arguments to `pptx_shape`
/// became one named bundle at its single call site.
pub(super) struct Shape<'a> {
    id: u32,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    texts: &'a [String],
    size: u32,
    bold: bool,
}

pub(super) fn pptx_shape(s: Shape<'_>) -> String {
    let ps = s
        .texts
        .iter()
        .map(|t| {
            format!(
                "<a:p><a:r><a:rPr sz=\"{}\"{} /><a:t>{}</a:t></a:r></a:p>",
                s.size * 100,
                if s.bold { " b=\"1\"" } else { "" },
                esc_xml(t)
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{}\" name=\"box{}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>{}</p:txBody></p:sp>", s.id, s.id, s.x, s.y, s.w, s.h, ps)
}

pub(super) fn pptx_slide(title: &str, bullets: &[String]) -> Vec<u8> {
    let mut texts = vec![title.to_string()];
    texts.extend(bullets.iter().map(|b| format!("{} {}", "\u{2022}", b)));
    let shape = pptx_shape(Shape {
        id: 2,
        x: 685800,
        y: 365760,
        w: 7772400,
        h: 4000000,
        texts: &texts,
        size: 20,
        bold: false,
    });
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>{}</p:spTree></p:cSld></p:sld>", shape).into_bytes()
}

pub(super) fn pptx_bytes(title: &str, slides: &[(String, Vec<String>)]) -> Vec<u8> {
    let all: Vec<(String, Vec<String>)> = if slides.is_empty() {
        vec![(title.to_string(), vec![])]
    } else {
        slides.to_vec()
    };
    let n = all.len();
    let mut z = ZipWriter::new();
    let mut overrides = String::new();
    for i in 1..=n {
        overrides.push_str(&format!("<Override PartName=\"/ppt/slides/slide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>", i));
    }
    z.file("[Content_Types].xml", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Types xmlns=\"{}\"><Default Extension=\"rels\" ContentType=\"{}\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>{}</Types>", "http://schemas.openxmlformats.org/package/2006/content-types", "application/vnd.openxmlformats-package.relationships+xml", overrides).as_bytes());
    z.file("_rels/.rels", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"{}\"><Relationship Id=\"rId1\" Type=\"{}\" Target=\"ppt/presentation.xml\"/></Relationships>", "http://schemas.openxmlformats.org/package/2006/relationships", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument").as_bytes());
    let sld_ids: String = (1..=n)
        .map(|i| format!("<p:sldId id=\"{}\" r:id=\"rId{}\" />", 255 + i, i))
        .collect::<Vec<_>>()
        .join("");
    let pres_rels: String = (1..=n)
        .map(|i| {
            format!(
                "<Relationship Id=\"rId{}\" Type=\"{}\" Target=\"slides/slide{}.xml\"/>",
                i, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide", i
            )
        })
        .collect::<Vec<_>>()
        .join("");
    z.file("ppt/presentation.xml", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:presentation xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><p:sldMasterIdLst><p:sldMasterId r:id=\"rIdMaster\"/></p:sldMasterIdLst><p:sldIdLst>{}</p:sldIdLst><p:sldSzCx cx=\"9144000\" cy=\"5143500\"/></p:presentation>", sld_ids).as_bytes());
    z.file("ppt/_rels/presentation.xml.rels", format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rIdMaster\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/>{}</Relationships>", pres_rels).as_bytes());
    z.file("ppt/slideMasters/slideMaster1.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sldMaster xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"1A1A1A\"/></a:solidFill></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz=\"3200\"/></a:lvl1pPr></p:titleStyle></p:txStyles></p:sldMaster>");
    z.file("ppt/slideLayouts/slideLayout1.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><p:sldLayout xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" type=\"titleAndContent\" preserve=\"1\"><p:cSld name=\"Title and Content\"><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld></p:sldLayout>");
    z.file("ppt/theme/theme1.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Argus\"><a:themeElements><a:clrScheme name=\"x\"><a:lt1><a:srgbClr val=\"FFFFFF\"/></a:lt1><a:dk1><a:srgbClr val=\"1A1A1A\"/></a:dk1></a:clrScheme></a:themeElements></a:theme>");
    z.file("docProps/core.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\"><cp:title>Argus</cp:title></cp:coreProperties>");
    z.file("docProps/app.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>Argus</Application></Properties>");
    for (i, (t, bullets)) in all.iter().enumerate() {
        let n = i + 1;
        z.file(
            &format!("ppt/slides/slide{}.xml", n),
            &pptx_slide(t, bullets),
        );
        z.file(
            &format!("ppt/slides/_rels/slide{}.xml.rels", n),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../../slideLayouts/slideLayout1.xml\"/></Relationships>".as_bytes(),
        );
    }
    z.finish()
}
