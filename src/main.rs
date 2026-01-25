use std::os::unix::net::UnixStream;
use std::env::var;
use std::io::{BufRead, BufReader};
use std::collections::HashMap; //for windows
use std::collections::BTreeMap; //Sort by id on insert

mod text;
use text::HELP;

struct AppArgs {
    all: bool,
    socket: Option<String>
}

fn main() {
 
let args = parse_args().expect("failed to parse args");

let sock:String = args.socket.clone().unwrap_or_else(|| 
        format!(
        "{}/hypr/{}/.socket2.sock",
        var("XDG_RUNTIME_DIR").unwrap(),
        var("HYPRLAND_INSTANCE_SIGNATURE").unwrap()
    )
)
;
let _ = readsock(&sock, args);


}

fn parse_args() -> Result<AppArgs, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let args = AppArgs { //value struct to be returned
        socket: pargs.opt_value_from_str([ "-p", "--path"])?,
        all: pargs.contains(["-a", "--all"]),
    };

    // It's up to the caller what to do with the remaining arguments.
    let remaining = pargs.finish();
    if !remaining.is_empty() {
        eprintln!("Warning: unused arguments left: {:?}.", remaining);
    }

    Ok(args)
}


fn hyprctl<T>(args: &[&str]) ->  Result<T, serde_json::Error> //Result is of a inputed type
where
    T: serde::de::DeserializeOwned, //Specify type restrictions
{
    //Comamnd output to collect
    let output = std::process::Command::new("hyprctl") 
        .args(args)
        .output().expect("failed to get hyprctl output"); //Result<Output, std::io::Error>
    Ok(serde_json::from_slice(&output.stdout)? ) // Result<T, serde_json::Error>
}



fn properties () -> Result<(u8, String,Vec<Workspace_D>,Vec<Client>), serde_json::Error> {

    let (workspace, win): (u8, String) = {
    //Deserialize "hyprctl activeworkspace -j" into Workspace struct
    let activework_out = hyprctl::<Workspace_D>(&["activeworkspace", "-j"])?; 
    //get id and lastwindowtitle for tupple of workspace and win
    (activework_out.id, activework_out.lastwindowtitle)
    };

    let workspaces:Vec<Workspace_D> = serde_json::from_value(hyprctl(&["workspaces", "-j"])?)?;

    let clients:Vec<Client>  = serde_json::from_value(hyprctl(&["clients", "-j"])?)?;


    Ok((workspace,win,workspaces,clients))


}



fn readsock(sock:&str, args:AppArgs) -> Result<(),std::io::Error> {

    //If both k and v could be hashed I wouldnt have to do this jank
           
    let mut by_id: HashMap<String, u8> = HashMap::new();
   
     let stream = UnixStream::connect(sock)?; // Result<UnixStream, std::io::Error>
     let reader = BufReader::new(stream);

     let (work,win,works,clients) = properties().expect("Hyprctl command failed to return results");
     let mut works1: BTreeMap<u8, Workspace> = works.into_iter()
       .map(|workspace_d| {
        (
            workspace_d.id,
            Workspace {
                name: workspace_d.name,
                lastwindowtitle: Some(workspace_d.lastwindowtitle),
                lastwindowid: None,
                windows_map: None,

            },
        )
    })
    .collect();

     clients.into_iter()
    .for_each(|client_d| {
        //The address from clients is 0xsomestring but IPC sends string without the 0x, So I just get rid of the 0x here
        let address:String = client_d.address[2..].to_string();
        //insert into works1
        if let Some(entry_struct) = works1.get_mut(&client_d.workspace.id) {
        entry_struct.lastwindowid = Some(client_d.address.clone().chars().skip(2).collect());
        entry_struct.windows_map
        .get_or_insert_with(HashMap::new)
        .insert(address.clone(), (client_d.title,client_d.class));

        }
        //insert into by_id
        by_id.insert(address, client_d.workspace.id);   
    }

    );

     let mut data = Data { active_win: win, active_win_id: "".to_string(), active_work: work, workspaces: works1};
     data.format();


     for line in reader.lines() {
     let line = line?; // Result<String, std::io::Error>
     
     if let Some((prefix, value)) = line.split_once(">>") {
         match prefix {
            //IPC Updates activewindow before workspace change message sent
            //activewindow>>kitty,~ //the window at workspace 3
            //workspace>>3 //changing to workspace 3

        "activewindow" => {
            data.active_win = value.to_string();
        }

        "activewindowv2" => { //activewindowv2>>55c018d12180
            data.active_win_id = value.to_string();
            data.format();
        }

        "workspacev2" => {
            
            let (name1, id) = value.split_once(',').expect("missing comma");
            let (name1, id):(String,u8) = (name1.to_string(),id.parse().expect("workspace id is not u8"));
            data.active_work = id;
            
            if let Some(entry_struct) = data.workspaces.get_mut(&id) { 
                entry_struct.name = name1;
                entry_struct.lastwindowtitle = Some(data.active_win.clone());
            }

            data.format();     

          }
        "createworkspacev2"  => { 
            
            let (name1, id) = value.split_once(',').expect("missing comma");
            let id:u8 = id.parse().expect("createworkspacev2 id is not a u8 number");
            data.workspaces.insert(id,
                Workspace { 
                    name: name1.to_string(), 
                    lastwindowtitle: None, 
                    lastwindowid: None,
                    windows_map: None
                }
                
            );

            data.format();
         }
        "destroyworkspacev2" => {            
            
            let (name1, id) = value.split_once(',').expect("missing comma");
            let id:u8 = id.parse().expect("destroyworkspacev2 id is not a u8 number");
            data.workspaces.remove(&id);
            data.format();

         }

        //    let by_id: HashMap<String, u8> = HashMap::new();
        //    let by_work: BTreeMap<u8, HashMap::<String,String> > = BTreeMap::new();
        // Id,Workspace
        // Workspace { {id,(Title,InitialClass)}  }
         "openwindow" => { // openwindow>>55c018ac1aa0,3,kitty,kitty
            
            let parts: Vec<&str> = value.split(',').collect();
            let [id, workspace, initialclass, initialtitle]: [&str; 4] = parts.try_into().expect("not 4 arguments in openwindow");
            
            let workspace: u8 = workspace.parse().expect("workspace in openwindow is not u8");
            let (id,initialclass, initialtitle): (String,String,String) = (id.to_string(),initialclass.to_string(),initialtitle.to_string());
            
            by_id.insert(id.clone(),workspace);
           
            if let Some(entry) = data.workspaces.get_mut(&workspace) {
             entry.windows_map.get_or_insert_with(HashMap::new)
            .insert(id,(initialtitle,initialclass));
            }

           data.format();
           }

           "closewindow" => { // closewindow>>55c018ac1aa0    
            let id:String = value.to_string();
            let work = by_id.get(&id).expect("closewindow: workspace id in by_key map is not a u8");

             if let Some(entry) = data.workspaces.get_mut(&work) {
                   if let Some(map) = entry.windows_map.as_mut() {
                        map.remove(&id).expect("this map doesn't have this id");
                    }
             };
            data.format();    
            }
            
            //windowtitle>>55c018be1280
            //windowtitlev2>>55c018be1280,kitty
            // openwindow>>55c018be1280,2,kitty,kitty
            // activewindow>>kitty,kitty
            //activewindowv2>>55c018be1280
            
            //windowtitlev2>>55c018cd24a0,LOVELY BASTARDS - YouTube — Mozilla Firefox
            "windowtitlev2" => {
                let (id,title) = value.split_once(',').expect("missing comma");
                //window title triggers before openwindow so workspace cant be gotten from the map
                if let Some(workspace) = by_id.get(id){ //.expect("window isint in workspace map yet");
                let title:String = title.to_string();
                if let Some(entry) = data.workspaces.get_mut(workspace) {
                    if let Some(map) = entry.windows_map.as_mut() {
                        if let Some((initialtitle,initialclass)) = map.get_mut(id) {
                        *initialtitle = title
                        }
                    }
                
                }
            } 
                

            data.format();
            }
            
            "movewindowv2"=> { //movewindowv2>>55c018d12180,4,4
                let parts: Vec<&str> = value.split(',').collect();
                let [id,workspace,workspace_name]: [&str; 3] = parts.try_into().expect("Wrong number of elements");
                let (id,workspace_name):(String,String) = (id.to_string(), workspace_name.to_string());
                let workspace:u8 = workspace.parse().expect("movewindowv2 workspace is invalid");

                if let Some(old_workspace) = by_id.insert(id.clone(), workspace) {
                     if let Some(entry) = data.workspaces.get_mut(&old_workspace) {                        
                        if let Some(map) = entry.windows_map.as_mut() {
                            let (initialtitle,initialclass) = map.remove(&id).expect("failed to remove old map during movewindowv2");
                            if let Some(workspace_entry) = data.workspaces.get_mut(&workspace) {
                                workspace_entry.windows_map.get_or_insert_with(HashMap::new).insert(id,(initialtitle,initialclass));
                            }
                        }
                    }
                }
            data.format();
            }

           
          _ => {}


     }
    }
    }
    Ok(())
}



struct Data {
    active_win: String,
    active_win_id: String,
    active_work: u8,
    workspaces: BTreeMap<u8, Workspace >
    //, Option< HashMap<String,(String,String)> >
}

struct Workspace { 
    name: String,
    lastwindowtitle: Option<String>, 
    lastwindowid: Option<String>,
    windows_map: Option< HashMap::<String,(String,String)> >
}

#[derive(serde::Deserialize)]
struct Workspace_D { 
    id: u8, //1 bytes // 7 bytes of padding to algin to 8 bytes
    name: String, // 3x 8 bytes
    lastwindowtitle: String //unessecary field for workspaces vector
}

#[derive(serde::Deserialize)]
struct Client_Work {
    id: u8,
    name: String,
}

#[derive(serde::Deserialize)]
struct Client {
    address: String,
    workspace: Client_Work,
    class: String,
    title: String,
}

impl Data {
fn format (&self) {

    let workspaces = self.workspaces.iter().map(|(index, value)| serde_json::json!({
        "id": index,
        "name": value.name,
        "lastwindowtitle": value.lastwindowtitle,
        "lastwindowid": value.lastwindowid,
        "windows": value.windows_map    
    }) )
        .collect::<Vec<_>>();
    
     

println!("\"workspaces\":{},\"active_window\":\"{}\",\"active_window_id\":\"{}\",\"active_workspace\":\"{}\"", 
serde_json::to_string(&workspaces).unwrap(),
self.active_win,
self.active_win_id,
self.active_work
);

}
}


