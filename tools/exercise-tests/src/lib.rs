// Exercise service compiled directly, without Tauri or personal data.
#[path="../../../src-tauri/src/exercise_svc.rs"]pub mod exercise_svc;
pub mod db {
use rusqlite::Connection;
pub fn get_setting(c:&Connection,k:&str)->Option<String>{c.query_row("SELECT value FROM settings WHERE key=?1",[k],|r|r.get(0)).ok()}
pub fn set_setting(c:&Connection,k:&str,v:&str)->Result<(),String>{c.execute("INSERT INTO settings(key,value)VALUES(?1,?2) ON CONFLICT(key)DO UPDATE SET value=excluded.value",[k,v]).map(|_|()).map_err(|e|e.to_string())}
}
pub mod engine {pub fn add_days(d:&str,n:i64)->String{(chrono::NaiveDate::parse_from_str(d,"%Y-%m-%d").unwrap()+chrono::Duration::days(n)).to_string()}}
pub mod store {pub fn now()->i64{1}}
pub mod engine_svc {pub struct Profile{pub tz_offset_s:i32}pub fn profile(_: &rusqlite::Connection)->Profile{Profile{tz_offset_s:0}}}
pub mod parse {pub mod time {pub fn ymd(t:i64,z:i32)->String{chrono::DateTime::from_timestamp(t+z as i64,0).unwrap().format("%Y-%m-%d").to_string()}}}
#[cfg(test)]mod tests{
use super::*;use serde_json::{json,Value};use rusqlite::Connection;
fn db()->Connection{let c=Connection::open_in_memory().unwrap();c.execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);CREATE TABLE strength_sets(activity_id TEXT,ex_id TEXT,reps INTEGER);INSERT INTO strength_sets VALUES('old','Pushups',12);").unwrap();c}
fn exercise()->Value{json!({"id":"c_fixture","ru":"Тяга ленты","eq":"bands","cat":"strength","p":["lats"],"s":[],"st":false,"leg":true,"bw":null,"purpose":["strength","ofp_running"],"measurement":"reps","side":"arm","media":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6LwAAAABJRU5ErkJggg==","mediaType":"illustration"})}
#[test]fn library_keeps_all_shipped_ids_and_load_coefficients(){
 exercise_svc::load_custom(&db());let l=exercise_svc::list();assert_eq!(l["items"].as_array().unwrap().len(),938);
 let mut d=flate2::read::GzDecoder::new(&include_bytes!("../../../src-tauri/resources/exercises.json.gz")[..]);let mut text=String::new();std::io::Read::read_to_string(&mut d,&mut text).unwrap();let old:Value=serde_json::from_str(&text).unwrap();
 let extra:Value=serde_json::from_str(include_str!("../../../src-tauri/resources/exercises_extra.json")).unwrap();
 for x in old["items"].as_array().unwrap().iter().chain(extra["items"].as_array().unwrap()) {let current=exercise_svc::by_id(x["id"].as_str().unwrap()).unwrap();for key in ["id","ru","tip","eq","p","s","bw","st","img"]{assert_eq!(current[key],x[key],"{}: {}",x["id"],key);}}
 assert_eq!(l["runnerOfp"],extra["runnerOfp"]);assert_eq!(l["bandList"],extra["bandList"]);
 assert_eq!(exercise_svc::by_id("new_kb_suitcase").unwrap()["side"],"arm");
 assert_eq!(exercise_svc::effective(&json!({"exId":"Pushups","weightKg":null}),Some(80.0),&[]).0,Some(51.2));
}
#[test]fn independent_save_never_touches_complexes_or_history_and_reopens_media(){let c=db();let original="[{\"id\":\"personal\",\"name\":\"Мой комплекс\",\"items\":[{\"exId\":\"Pushups\"}]}]";db::set_setting(&c,"complexes",original).unwrap();let saved=exercise_svc::save_custom(&c,&exercise()).unwrap();assert_eq!(saved["side"],"arm");assert_eq!(db::get_setting(&c,"complexes").unwrap(),original);assert_eq!(c.query_row("SELECT reps FROM strength_sets",[],|r|r.get::<_,i32>(0)).unwrap(),12);exercise_svc::load_custom(&c);assert_eq!(exercise_svc::custom_snapshot()[0]["media"],exercise()["media"]);let stored:Value=serde_json::from_str(&db::get_setting(&c,"custom_exercises").unwrap()).unwrap();assert_eq!(stored[0]["purpose"],exercise()["purpose"]);}
#[test]fn old_complex_editor_retains_new_metadata_and_concurrently_added_exercises(){let c=db();exercise_svc::save_custom(&c,&exercise()).unwrap();let old_form=json!({"id":"c_fixture","ru":"Новое имя","tip":"техника","p":["lats"],"s":[],"st":false,"leg":true,"bw":null});exercise_svc::save_complexes(&c,&[],&[old_form]).unwrap();let v:Value=serde_json::from_str(&db::get_setting(&c,"custom_exercises").unwrap()).unwrap();assert_eq!(v[0]["media"],exercise()["media"]);assert_eq!(v[0]["eq"],"bands");assert_eq!(v[0]["purpose"],exercise()["purpose"]);assert_eq!(v[0]["side"],"arm");exercise_svc::save_complexes(&c,&[],&[]).unwrap();let v:Value=serde_json::from_str(&db::get_setting(&c,"custom_exercises").unwrap()).unwrap();assert_eq!(v.as_array().unwrap().len(),1);}
#[test]fn favorites_are_durable_unique_and_do_not_mutate_workouts(){let c=db();exercise_svc::set_favorite(&c,"Pushups",true).unwrap();exercise_svc::set_favorite(&c,"Pushups",true).unwrap();assert_eq!(exercise_svc::favorites(&c),vec!["Pushups"]);assert!(exercise_svc::set_favorite(&c,"missing",true).is_err());assert!(exercise_svc::set_favorite(&c,"Pushups",false).unwrap().is_empty());assert_eq!(c.query_row("SELECT count(*) FROM strength_sets",[],|r|r.get::<_,i32>(0)).unwrap(),1);}
#[test]fn invalid_media_or_category_leaves_existing_settings_intact(){let c=db();db::set_setting(&c,"custom_exercises","[]").unwrap();let mut x=exercise();x["media"]=json!("https://example.com/image.svg");assert!(exercise_svc::save_custom(&c,&x).is_err());x=exercise();x["purpose"]=json!([]);assert!(exercise_svc::save_custom(&c,&x).is_err());assert_eq!(db::get_setting(&c,"custom_exercises").unwrap(),"[]");}
#[test]fn corrupt_existing_custom_data_is_never_overwritten(){let c=db();let original="{broken data";db::set_setting(&c,"custom_exercises",original).unwrap();assert!(exercise_svc::save_custom(&c,&exercise()).is_err());assert!(exercise_svc::save_complexes(&c,&[],&[]).is_err());assert_eq!(db::get_setting(&c,"custom_exercises").unwrap(),original);}

#[test]fn legacy_checkboxes_do_not_reenable_time_or_one_sided_mode(){let c=db();let mut original=exercise();original["measurement"]=json!("seconds");original["st"]=json!(true);exercise_svc::save_custom(&c,&original).unwrap();let old_form=json!({"id":"c_fixture","ru":"Тяга ленты","tip":"техника","p":["lats"],"s":[],"st":false,"leg":false,"bw":null});exercise_svc::save_complexes(&c,&[],&[old_form]).unwrap();let saved:Value=serde_json::from_str(&db::get_setting(&c,"custom_exercises").unwrap()).unwrap();assert_eq!(saved[0]["measurement"],"reps");assert!(saved[0]["side"].is_null());assert!(!exercise_svc::one_sided("c_fixture"));assert_eq!(saved[0]["media"],original["media"]);assert_eq!(saved[0]["purpose"],original["purpose"]);}

}
