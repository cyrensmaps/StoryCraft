//! Package manifests for the LMS formats: SCORM 1.2 and 2004
//! (`imsmanifest.xml`), xAPI (`tincan.xml`) and cmi5 (`cmi5.xml`).

use storycraft_model::Project;

use crate::html_escape as xml_escape;

/// The course's IRI for xAPI and cmi5. An author-supplied course ID that is
/// already an IRI (has a scheme, e.g. `https://example.com/courses/safety`)
/// is used as-is; anything else becomes a `urn:storycraft:course:…` IRI.
pub fn activity_id(project: &Project) -> String {
    let course_id = project.lms.course_id.trim();
    if course_id.contains(':') && !course_id.contains(char::is_whitespace) {
        return course_id.to_owned();
    }
    let raw = if course_id.is_empty() { project.id.to_string() } else { course_id.to_owned() };
    let safe: String =
        raw.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_') { c } else { '-' }).collect();
    format!("urn:storycraft:course:{safe}")
}

fn course_identifier(project: &Project) -> String {
    let id = project.lms.course_id.trim();
    xml_id(if id.is_empty() { project.id.as_str() } else { id })
}

/// Make a string usable as an XML `ID`: a letter first, then only letters,
/// digits, `-`, `_` and `.`.
fn xml_id(s: &str) -> String {
    let body: String =
        s.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect();
    format!("SC-{body}")
}

fn file_list(files: &[&str]) -> String {
    files.iter().map(|f| format!("      <file href=\"{}\"/>\n", xml_escape(f))).collect()
}

/// SCORM 1.2: a single SCO listing every file in the package.
pub fn scorm12(project: &Project, files: &[&str]) -> String {
    let identifier = course_identifier(project);
    let title = xml_escape(&project.title);
    let files = file_list(files);
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
{files}    </resource>
  </resources>
</manifest>
"#
    )
}

/// SCORM 2004 4th edition: a single SCO with default sequencing.
pub fn scorm2004(project: &Project, files: &[&str]) -> String {
    let identifier = course_identifier(project);
    let title = xml_escape(&project.title);
    let files = file_list(files);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest identifier="{identifier}" version="1"
  xmlns="http://www.imsglobal.org/xsd/imscp_v1p1"
  xmlns:adlcp="http://www.adlnet.org/xsd/adlcp_v1p3"
  xmlns:adlseq="http://www.adlnet.org/xsd/adlseq_v1p3"
  xmlns:adlnav="http://www.adlnet.org/xsd/adlnav_v1p3"
  xmlns:imsss="http://www.imsglobal.org/xsd/imsss"
  xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
  xsi:schemaLocation="http://www.imsglobal.org/xsd/imscp_v1p1 imscp_v1p1.xsd http://www.adlnet.org/xsd/adlcp_v1p3 adlcp_v1p3.xsd http://www.adlnet.org/xsd/adlseq_v1p3 adlseq_v1p3.xsd http://www.adlnet.org/xsd/adlnav_v1p3 adlnav_v1p3.xsd http://www.imsglobal.org/xsd/imsss imsss_v1p0.xsd">
  <metadata>
    <schema>ADL SCORM</schema>
    <schemaversion>2004 4th Edition</schemaversion>
  </metadata>
  <organizations default="ORG-1">
    <organization identifier="ORG-1">
      <title>{title}</title>
      <item identifier="ITEM-1" identifierref="RES-1">
        <title>{title}</title>
      </item>
    </organization>
  </organizations>
  <resources>
    <resource identifier="RES-1" type="webcontent" adlcp:scormType="sco" href="index.html">
{files}    </resource>
  </resources>
</manifest>
"#
    )
}

/// xAPI (Tin Can) package descriptor.
pub fn tincan(project: &Project) -> String {
    let id = xml_escape(&activity_id(project));
    let title = xml_escape(&project.title);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<tincan xmlns="http://projecttincan.com/tincan.xsd">
  <activities>
    <activity id="{id}" type="http://adlnet.gov/expapi/activities/course">
      <name>{title}</name>
      <description lang="und">{title}</description>
      <launch lang="und">index.html</launch>
    </activity>
  </activities>
</tincan>
"#
    )
}

/// cmi5 course structure with a single AU. The AU completes the course as
/// soon as it reports completion.
pub fn cmi5(project: &Project) -> String {
    let course = activity_id(project);
    let au = xml_escape(&format!("{course}/au"));
    let course = xml_escape(&course);
    let title = xml_escape(&project.title);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<courseStructure xmlns="https://w3id.org/xapi/profiles/cmi5/v1/CourseStructure.xsd">
  <course id="{course}">
    <title><langstring lang="en-US">{title}</langstring></title>
    <description><langstring lang="en-US">{title}</langstring></description>
  </course>
  <au id="{au}" moveOn="Completed" launchMethod="AnyWindow">
    <title><langstring lang="en-US">{title}</langstring></title>
    <description><langstring lang="en-US">{title}</langstring></description>
    <url>index.html</url>
  </au>
</courseStructure>
"#
    )
}

#[cfg(test)]
mod tests {
    use storycraft_model::Project;

    use super::*;

    #[test]
    fn ids_are_sanitised() {
        assert_eq!(xml_id("my course/1"), "SC-my_course_1");
    }

    #[test]
    fn activity_ids() {
        let mut p = Project::new("x");
        p.lms.course_id = "https://example.com/courses/safety".into();
        assert_eq!(activity_id(&p), "https://example.com/courses/safety");
        p.lms.course_id = "Fire safety 101".into();
        assert_eq!(activity_id(&p), "urn:storycraft:course:Fire-safety-101");
        p.lms.course_id.clear();
        assert!(activity_id(&p).starts_with("urn:storycraft:course:"));
    }

    #[test]
    fn manifests_are_well_formed_xml() {
        let mut p = Project::new("Safety & <Health>");
        p.lms.course_id = "https://example.com/c?a=1&b=2".into();
        let files = ["index.html", "media/a&b.png"];
        for xml in [scorm12(&p, &files), scorm2004(&p, &files), tincan(&p), cmi5(&p)] {
            let mut reader = quick_xml::Reader::from_str(&xml);
            loop {
                match reader.read_event() {
                    Ok(quick_xml::events::Event::Eof) => break,
                    Ok(_) => {}
                    Err(e) => panic!("{e}\n{xml}"),
                }
            }
            assert!(xml.contains("Safety &amp; &lt;Health&gt;"));
        }
    }
}
