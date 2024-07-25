use metaverse_login::login::{build_struct_with_defaults, login_with_defaults};
use metaverse_session::models::errors::Reason;
use metaverse_session::models::session_data::AgentAccess;
use metaverse_session::session::{connect, new_session};

use std::collections::HashMap;
use std::net::TcpStream;
use std::panic;
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::{Duration, Instant};
use uuid::Uuid;

const PYTHON_PORT: u16 = 9000;
const PYTHON_URL: &str = "http://127.0.0.1";
const OSGRID_PORT: u16 = 80;
const OSGRID_URL: &str = "http://login.osgrid.org";
const THIRDROCK_PORT: u16 = 8002;
const THIRDROCK_URL: &str = "http://grid.3rdrockgrid.com";

struct Reap(Child);
impl Drop for Reap {
    fn drop(&mut self) {
        self.0.kill().expect("process already died");
    }
}

fn init_logger() {
    let _ = env_logger::builder()
        .filter(None, LevelFilter::Info)
        .is_test(true)
        .try_init();
}

#[test]
fn test_mock_session() {
    let mut reaper = match setup() {
        Ok(reap) => reap,
        Err(_string) => return,
    };
    match reaper.0.try_wait().unwrap() {
        None => {}
        Some(status) => {
            panic!("python process unexpectedly exited: {}", status);
        }
    }

    let login_response = login_with_defaults(
        env!("CARGO_CRATE_NAME").to_string(),
        "first".to_string(),
        "last".to_string(),
        "password".to_string(),
        "last".to_string(),
        true,
        true,
        build_test_url(PYTHON_URL, PYTHON_PORT),
    );

    assert_eq!(
        session.home.unwrap().region_handle,
        ("r0".to_string(), "r0".to_string())
    );
    assert_eq!(
        session.look_at,
        Some(("r0".to_string(), "r0".to_string(), "r0".to_string()))
    );
    assert_eq!(session.agent_access, Some(AgentAccess::Mature));
    assert_eq!(session.agent_access_max, Some(AgentAccess::Adult));
    assert_eq!(
        session.seed_capability,
        Some("http://192.168.1.2:9000".to_string())
    );
    assert_eq!(session.first_name, Some("First".to_string()));
    assert_eq!(session.last_name, Some("Last".to_string()));
    assert_eq!(
        session.agent_id,
        Some(Uuid::parse_str("11111111-1111-0000-0000-000100bba000").unwrap())
    );
    assert_eq!(session.sim_ip, Some("192.168.1.2".to_string()));
    assert_eq!(session.sim_port, Some(9000));
    assert_eq!(session.http_port, Some(0));
    assert_eq!(session.start_location, Some("last".to_string()));
    assert_eq!(session.region_x, Some(256000));
    assert_eq!(session.region_y, Some(256000));
    assert_eq!(session.circuit_code, Some(697482820));
    assert_eq!(
        session.session_id,
        Some(Uuid::parse_str("6ac2e761-f490-4122-bf6c-7ad8fbb17002").unwrap())
    );
    assert_eq!(
        session.secure_session_id,
        Some(Uuid::parse_str("fe210274-9056-467a-aff7-d95f60bacccc").unwrap())
    );
    assert_eq!(
        session.inventory_root.unwrap()[0].folder_id,
        "37c4cfe3-ea39-4ef7-bda3-bee73bd46d95".to_string()
    );
    let inv_skel = session.inventory_skeleton.unwrap();
    assert_eq!(inv_skel.len(), 2);
    assert_eq!(
        inv_skel[0].folder_id,
        "004d663b-9980-46ae-8559-bb60e9d67d28".to_string()
    );
    assert_eq!(
        session.inventory_lib_root.unwrap()[0].folder_id,
        "37c4cfe3-ea39-4ef7-bda3-bee73bd46d95".to_string()
    );
    let inv_skel_lib = session.inventory_skeleton_lib.unwrap();
    assert_eq!(inv_skel_lib.len(), 2);
    assert_eq!(
        inv_skel_lib[0].folder_id,
        "004d663b-9980-46ae-8559-bb60e9d67d28".to_string()
    );
    assert_eq!(
        session.inventory_lib_owner.unwrap()[0].agent_id,
        ("11111111-1111-0000-0000-000100bba000").to_string()
    );
    assert_eq!(
        session.map_server_url,
        Some("http://192.168.1.2:8002/".to_string())
    );

    let buddy_list = session.buddy_list.unwrap();
    assert_eq!(buddy_list.len(), 3);
    assert_eq!(
        buddy_list[0].buddy_id,
        "04c259b7-94bc-4822-b099-745191ffc247".to_string()
    );
    assert!(buddy_list[0].buddy_rights_given.can_see_online);

    let gesture_list = session.gestures.unwrap();
    assert_eq!(gesture_list.len(), 2);
    assert_eq!(
        gesture_list[0].item_id,
        "004d663b-9980-46ae-8559-bb60e9d67d28".to_string()
    );
    assert_eq!(
        gesture_list[0].asset_id,
        "004d663b-9980-46ae-8559-bb60e9d67d28".to_string()
    );
    assert_eq!(
        session.initial_outfit.unwrap()[0].folder_name,
        "Nightclub Female".to_string()
    );
    assert_eq!(
        session.global_textures.unwrap()[0].sun_texture_id,
        "cce0f112-878f-4586-a2e2-a8f104bba271".to_string()
    );
    assert!(session.login.unwrap());
    assert_eq!(
        session.login_flags.unwrap()[0].seconds_since_epoch,
        Some(1411075065)
    );
    assert_eq!(session.message.unwrap(), "Welcome, Avatar!".to_string());
    assert!(session.ui_config.unwrap()[0].allow_first_life);
    assert_eq!(
        session.classified_categories.unwrap()[0].category_name,
        "Shopping".to_string()
    );

    match reaper.0.try_wait().unwrap() {
        None => {}
        Some(status) => {
            panic!("python process unexpectedly exited: {}", status);
        }
    }
}

#[test]
async fn test_local(){
    init_logger();

    let notify = Arc::new(Notify::new());
    let state = Arc::new(Mutex::new(ServerState::Starting));

    // start the sim server, and initialize logging
    let sim_server = setup_server(Arc::clone(&notify), Arc::clone(&state)).await;
    send_setup_commands(&sim_server);

    notify.notified().await;
    if *state.lock().unwrap() == ServerState::Running {
        info!("Server started. Running test commands");
        sim_server.do_send(CommandMessage{
            command: "create user default user password email@email.com 9dc18bb1-044f-4c68-906b-2cb608b2e197 default".to_string()
        });

        tokio::task::spawn_blocking(|| {
            let session = new_session(
                Login{first: "default".to_string(), 
                    last:"user".to_string(),
                passwd: "password".to_string(),
                start: "home".to_string(),
                channel: "benthic".to_string(),
                agree_to_tos: true, 
                read_critical: true}, build_test_url("http://127.0.0.1", 9000)
            );

        });
        sleep(Duration::from_secs(5)).await;
        sim_server.do_send(CommandMessage {
            command: "quit".to_string(),
        });
    } else {
        panic!("server failed to start")
    }

    notify.notified().await;

    new_session(login_data, login_url)

}


fn send_setup_commands(sim_server: &actix::Addr<SimServer>) {
    // This is required for first time startup. This assigns the default user as the region owner.
    // TODO: make this into a sql query that automatically adds this to the default region on
    // startup
    sim_server.do_send(CommandMessage {
        command: "default".to_string(),
    });
    sim_server.do_send(CommandMessage {
        command: "user".to_string(),
    });
    sim_server.do_send(CommandMessage {
        command: "password".to_string(),
    });
    sim_server.do_send(CommandMessage {
        command: "email@email.com".to_string(),
    });
    sim_server.do_send(CommandMessage {
        command: "9dc18bb1-044f-4c68-906b-2cb608b2e197".to_string(),
    });
}

async fn setup_server(
    notify: Arc<Notify>,
    state: Arc<Mutex<ServerState>>,
) -> actix::Addr<SimServer> {
    let (stdin_sender, stdin_receiver) = mpsc::channel::<CommandMessage>(100);
    let (stdout_sender, mut receiver) = mpsc::channel::<StdoutMessage>(100);

    let (url, archive, base_dir, executable) = match read_config() {
        Ok((url, archive, base_dir, executable)) => (url, archive, base_dir, executable),
        Err(e) => panic!("Error: {}", e),
    };

    info!("downloading server. On first run, this may take a while");
    match download_sim(&url, &archive, &base_dir).await {
        Ok(_) => info!("downloaded sim successfully"),
        Err(e) => info!("failed to download sim {}", e),
    };

    let sim_server = SimServer {
        state: Arc::clone(&state),
        sim_config: create_default_config(),
        standalone_config: create_default_standalone_config(),
        regions_config: create_default_region_config(),
        process: None,
        process_stdout_sender: Some(stdout_sender),
        process_stdin_receiver: Some(stdin_receiver),
        process_stdin_sender: Some(stdin_sender),
        notify: Arc::clone(&notify),
        exec_data: ExecData {
            base_dir,
            executable,
            init_command: "mono".to_string(),
        },
    }
    .start();
    info!("Waiting for the server to start...");

    tokio::spawn(async move {
        loop {
            if let Some(msg) = receiver.recv().await {
                info!("Received message: {}", msg.log_content);
                if msg.log_content.contains("Currently selected region is") {}
            }
        }
    });
    sim_server
}

fn read_creds() -> Option<HashMap<String, String>> {
    let mut settings = config::Config::default();
    match settings.merge(config::File::with_name(".creds")) {
        Ok(_file) => _file,
        Err(..) => {
            return None;
        }
    };
    settings
        .merge(config::Environment::with_prefix("APP"))
        .unwrap();

    Some(settings.try_into::<HashMap<String, String>>().unwrap())
}

/// helper function for building URL. May be unnescecary
fn build_test_url(url: &str, port: u16) -> String {
    let mut url_string = "".to_owned();
    url_string.push_str(url);
    url_string.push(':');
    url_string.push_str(&port.to_string());
    println!("url string {}", url_string);
    url_string
}

/// sets up python xmlrpc server for testing
fn setup() -> Result<Reap, String> {
    // logs when server started
    let start = Instant::now();
    // runs the python command to start the test server
    let mut child = match Command::new("python3")
        .arg("tests/test_server/test_server.py")
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            eprintln!("could not start test server, ignoring test({})", e);
            return Err("Could not start test server".to_string());
        }
    };

    // logs how many tries it took to connect to server
    // attempts to connect to python server
    for iteration in 0.. {
        match child.try_wait().unwrap() {
            None => {}
            Some(status) => panic!("python process died {}", status),
        }
        match TcpStream::connect(("localhost", PYTHON_PORT)) {
            Ok(_) => {
                println!(
                    "connected to server after {:?} (iteration{})",
                    Instant::now() - start,
                    iteration
                );
                return Ok(Reap(child));
            }
            Err(_) => {}
        }
        sleep(Duration::from_millis(50));
    }
    Ok(Reap(child))
}
