//! SCORM 1.2 content packaging (`imsmanifest.xml`).

use storycraft_model::Project;

use crate::html_escape as xml_escape;

/// A single-SCO manifest listing every file in the package.
pub fn manifest(project: &Project, files: &[&str]) -> String {
    let course_id =
        if project.lms.course_id.trim().is_empty() { project.id.to_string() } else { project.lms.course_id.clone() };
    let identifier = xml_id(&course_id);
    let title = xml_escape(&project.title);
    let file_list: String = files.iter().map(|f| format!("      <file href=\"{}\"/>\n", xml_escape(f))).collect();

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest identifier="{identifier}" version="1.0"
  xmlns="http://www.imsproject.org/xsd/imscp_rootv1p1p2"
  xmlns:adlcp="http://www.adlnet.org/xsd/adlcp_rootv1p2"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  xsi:schemaLocation="http://www.imsproject.org/xsd/imscp_rootv1p1p2 imscp_rootv1p1p2.xsd http://www.imsglobal.org/xsd/imsmd_rootv1p2p1 imsmd_rootv1p2p1.xsd http://www.adlnet.org/xsd/adlcp_rootv1p2 adlcp_rootv1p2.xsd">
  <metadata>
    <schema>ADL SCORM</schema>
    <schemaversion>1.2</schemaversion>
  </metadata>
  <organizations default="ORG-1">
    <organization identifier="ORG-1">
      <title>{title}</title>
      <item identifier="ITEM-1" identifierref="RES-1" isvisible="true">
        <title>{title}</title>
      </item>
    </organization>
  </organizations>
  <resources>
    <resource identifier="RES-1" type="webcontent" adlcp:scormtype="sco" href="index.html">
{file_list}    </resource>
  </resources>
</manifest>
"#
    )
}

/// Make a string usable as an XML `ID`: a letter first, then only letters,
/// digits, `-`, `_` and `.`.
fn xml_id(s: &str) -> String {
    let body: String =
        s.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect();
    format!("SC-{body}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn ids_are_sanitised() {
        assert_eq!(super::xml_id("my course/1"), "SC-my_course_1");
    }
}
