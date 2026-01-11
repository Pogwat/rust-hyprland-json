use std::os::unix::net::UnixStream;
use std::env;
use std::path::Path;
use std::io::{BufReader, prelude::*};
use std::collections::BTreeMap; //Sort by id on insert
use std::process::Command;

fn main() {
 readsock2(Path::new(&env::var_os("XDG_RUNTIME_DIR").unwrap())
 //Join bunch of Strings into single Path
 .join("hypr")
 .join(env::var_os("HYPRLAND_INSTANCE_SIGNATURE").unwrap())
 .join(".socket2.sock")).ok();
}

//Hyprctl helper fucntion with type formating and specfiying and input arguments i.e. Workspace struct and hyprctl commands
fn hyprctl<T>(args: &[&str]) -> Result<T, Box<dyn std::error::Error>> //Result is of a inputed type
where
    T: serde::de::DeserializeOwned, //Specify type restrictions
{
    //Comamnd output to collect
    let output = Command::new("hyprctl")
        .args(args)
        .output()?;
    Ok(serde_json::from_slice(&output.stdout)? ) //function returns rust code that deserializes comamnd output
}
//All struct
struct All {
    active_win: String,
    active_work: u8,
    workspaces: BTreeMap<u8, String>
}

//Computed Json String Cache
struct All_Json {
    active_work_j: String,
    computed_workspaces_j: String
}

impl All {
//At the moment all this is supper inefficent, but i will make it better :)
  
    fn Desc_Works_J(&self, all_json: &mut All_Json){
     let output: String = 
     self.workspaces //all_json.workpsaces_j is Vec<String>
     .iter().map(|(id, name)| format!("{{\"id\":\"{}\",\"name\":\"{}\"}}", id, name)) //function on iterator, only eval when consumed by collect, repeat until iter fully consumed
     .collect::<Vec<_>>().join(", "); //consumes function output and iter into the vector

     all_json.computed_workspaces_j = format!("\"workspaces\": [{}]"  ,&output)
    }

    fn Desc_A_Work_J(&self, all_json: &mut All_Json){
     all_json.active_work_j = format!("\"current_workspace\":\"{}\""
     ,self.active_work)
    }

}

impl All_Json {

    fn Rebuild(&self, all: &mut All){
     println!( " {{ \"active_window\":\"{}\",{},{} }}"
     ,all.active_win
     ,self.active_work_j
     ,self.computed_workspaces_j
     )
    }

}

fn readsock2 <P: AsRef<Path>>(socket_path: P) -> Result<(), Box<dyn std::error::Error>> {
    
    //Socket Path
    let stream = UnixStream::connect(socket_path)?;
    //Bufferd read into socket
    let reader = BufReader::new(&stream);

    //Struct hyprctl output is deserialized into
    #[derive(serde::Deserialize)]
    struct Workspace { 
        id: u8, //1 bytes // 7 bytes of padding to algin to 8 bytes
        name: String, // 3x 8 bytes
        lastwindowtitle: String //unessecary field for workspaces vector
    }

    let (workspace, win): (u8, String) = {
    //Deserialize "hyprctl activeworkspace -j" into Workspace struct
    let activework_out = hyprctl::<Workspace>(&["activeworkspace", "-j"])?; 
    //get id and lastwindowtitle for tupple of workspace and win
    (activework_out.id, activework_out.lastwindowtitle)
    };

    //declare struct for All Values
    let mut my_struct = All  { active_win: win, active_work: workspace, workspaces: BTreeMap::new()};

    //iterate over hyprctl json array  deserialze into vector of struct type so only get id: as u8 and name: as String 
    let workspaces:Vec<Workspace> = serde_json::from_value(hyprctl(&["workspaces", "-j"])?)?;

    for &Workspace { id, ref name, .. } in &workspaces {
    my_struct.workspaces.insert(id, name.to_string() ); 
    }

    let mut my_json_struct = All_Json {active_work_j: String::new(),computed_workspaces_j: String::new() };
    
    my_struct.Desc_Works_J(&mut my_json_struct);
    my_struct.Desc_A_Work_J(&mut my_json_struct);
    
    //Intital Describe before reading from socket
    my_json_struct.Rebuild(&mut my_struct);

    //Read lines from Socket BufferReader
    for line_result in reader.lines() {
    let line = line_result?;
   
    //Line Patern Matching and value insertion into All Struct

    if let Some((prefix, value)) = line.split_once(">>") {

      match prefix {
         "activewindow" => {
              my_struct.active_win = value.to_string();
              my_json_struct.Rebuild(&mut my_struct); // Print Values of struct with describe
          }
         "workspace" => {
             my_struct.active_work = value.parse::<u8>()?;
             my_struct.Desc_A_Work_J(&mut my_json_struct);
             my_json_struct.Rebuild(&mut my_struct);
          }
         "createworkspacev2" => {
              let (name, id) = value.split_once(',').expect("missing comma");
              my_struct.workspaces.insert(id.parse::<u8>()?,name.to_string());
              my_struct.Desc_Works_J(&mut my_json_struct);
              my_json_struct.Rebuild(&mut my_struct);
         }
         "destroyworkspace" => {
              my_struct.workspaces.remove(&value.parse::<u8>()?);
              my_struct.Desc_Works_J(&mut my_json_struct);
              my_json_struct.Rebuild(&mut my_struct);
         }
          _ => {}
}}}
Ok(()) //Return succes and end function
} 