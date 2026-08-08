use crate::llm::types::{ImagePart, Message};

#[test]
fn images_become_base64_blocks() {
    let m = Message::user_with_images("看图", vec![ImagePart { media_type: "image/png".into(), data: "QUJD".into() }]);
    let v = super::wire_content(&m);
    let arr = v.as_array().unwrap();
    assert_eq!(arr[0]["type"], "image");
    assert_eq!(arr[0]["source"]["media_type"], "image/png");
    assert_eq!(arr[0]["source"]["data"], "QUJD");
    assert_eq!(arr[1]["type"], "text");
}

#[test]
fn no_images_stays_plain_string() {
    let m = Message::user("hello");
    assert!(super::wire_content(&m).is_string());
}
