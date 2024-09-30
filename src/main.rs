use evdev_rs::{enums::{BusType, EventCode, EventType, EV_KEY, EV_REL, EV_SYN}, AbsInfo, DeviceWrapper, EnableCodeData, InputEvent, TimeVal, UInputDevice, UninitDevice};
use rusb::{Device, Context, DeviceDescriptor, DeviceHandle, UsbContext};
use std::{time::Duration, thread::sleep, sync::{Arc, atomic::{AtomicBool, Ordering}}};
use std::io::{BufReader, BufRead};
use std::collections::HashMap;
use core::str::FromStr;

const VENDOR: u16 = 0x08f2;
const DEVICE: u16 = 0x6811;

const MAX_PRESSURE: f32 = 2048.;

//static mut SENSIVITY: i16 = 256;
//static mut INVERSE_X: bool = false;
//static mut INVERSE_Y: bool = false;
//static mut SWAP: bool = false;

struct AppState<'a> {
	sensivity: i16, inv_x: bool, inv_y: bool, swap: bool, buttons: HashMap<&'a str, Vec<String>>, button_clicks: [i32; 14],
}

fn main() {
	std::env::set_var("RUST_BACKTRACE", "0");
	
	let args: Vec<String> = std::env::args().collect();
	let mut preset_path: String = String::from("");
	
	let mut state = AppState {
		sensivity: 256,
		inv_x: false,
		inv_y: false,
		swap: false,
		buttons: HashMap::new(),
		button_clicks: [0;14]
	};
	
	for param in args {
		if param.contains("preset=") {
			preset_path = param.replace("preset=", "").replace("\"", "");
			println!("Using preset file by path \"{}\"", preset_path);
		}//else { println!("Found {}", param); }
	}
	match std::fs::File::open(preset_path) {
		Ok(file)=>{
			let reader = BufReader::new(file);
			for set in reader.lines() {
				match set {
					Ok(rule)=>{
						if rule.contains("swap") {
							state.swap = rule.replace("swap=", "").contains("true");
							if state.swap { println!("SWAP_SET=TRUE, using YX ordering..") }
							else { println!("SWAP_SET=FALSE, using XY ordering (classic)..")}
						}else if rule.contains("sensivity") {
							let word = rule.replace("sensivity=", "");
							let val: &str = word.as_str();
							match val.parse::<i16>() {
								Ok(v) => {
									state.sensivity = v;
									println!("Pick uping SENSIVITY_SET ok with preset {:?}..", v)
								}, Err(e)=> println!("Pick uping SENSIVITY_SET failed with error {:?}, skip this rule", e)
							}
						}else if rule.contains("inverse") {
							let word = rule.replace("inverse=", "");
							let data: Vec<&str> = word.split(";").collect();
							state.inv_x = String::from(data[0]).contains("true");
							state.inv_y = String::from(data[1]).contains("true");
							println!("INVERSE_X_SET={} and INVERSE_Y_SET={}", state.inv_x, state.inv_y);
						}else if rule.contains("penbinds") {
							let word = rule.replace("penbinds=", "");
    						let vpendata: Vec<&str> = word.split(";").collect(); //.map(|v| v.to_string())
							for binds in &vpendata {
								let unpacked = binds.split(":").collect::<Vec<&str>>()[1];
								let aliases: Vec<String> = unpacked.split("+").map(|v| v.to_string()).collect();
								if binds.contains("VPEN_PLUS") {
									println!("Picked_UP {:?}", &aliases);
									state.buttons.insert("VPEN_PLUS", aliases);
								}else if binds.contains("VPEN_MINUS") {
									state.buttons.insert("VPEN_MINUS", aliases);
								}
							}
						}else if rule.contains("keybinds") {
							let word = rule.replace("keybinds=", "");
							let keydata: Vec<&str> = word.split(";").collect();
							for binds in keydata {
								let inf: Vec<&str> = binds.split(":").collect();
								if inf.len() != 2 { println!("Bad rule word [{}], skip this", binds); continue; }
								let unpacked = inf[1];
								let aliases: Vec<String> = unpacked.split("+").map(|v| v.to_string()).collect();
								match inf[0] {
									"KEY_L1" => { state.buttons.insert("KEY_L1", aliases); },
									"KEY_L2"=> { state.buttons.insert("KEY_L2", aliases); },
									"KEY_L3"=> { state.buttons.insert("KEY_L3", aliases); },
									"KEY_L4"=> { state.buttons.insert("KEY_L4", aliases); },
									"KEY_L5"=> { state.buttons.insert("KEY_L5", aliases); },
									"KEY_L6"=> { state.buttons.insert("KEY_L6", aliases); },
									
									"KEY_R1" => { state.buttons.insert("KEY_R1", aliases); },
									"KEY_R2"=> { state.buttons.insert("KEY_R2", aliases); },
									"KEY_R3"=> { state.buttons.insert("KEY_R3", aliases); },
									"KEY_R4"=> { state.buttons.insert("KEY_R4", aliases); },
									"KEY_R5"=> { state.buttons.insert("KEY_R5", aliases); },
									"KEY_R6"=> { state.buttons.insert("KEY_R6", aliases); },
									_=>{},
								}
							}
						}
				}, Err(e)=> println!("Bad rule word {:?}, skip this line.", e)
				}
			}
		},
		Err(e)=>{
			println!("Using default settings. Cannot read preset file. Obtained error {:?}", e);
		}
	}
	
	println!("Welcome to DEXP Tablet Ombra M Driver Shell!");
	print!("Try to found LIBUSB_LIBRARY...");
	match Context::new() {
		Ok(mut sys) => {
			print!("Try to found target device with {:04x}:{:04x}...", VENDOR, DEVICE);
			match catch_tablet(&mut sys, VENDOR, DEVICE) {
				None => println!(" CANCELLED!\nInfo: Device(s) not found!"),
				Some((_, _, devhandl)) => {
					println!(" Ok!");
					let _ = devhandl.claim_interface(2);
					
					detach_kernel_support( &devhandl);
					enter_advance_mode(&devhandl);
					start_listener(&devhandl, &mut state);
					
					let _ = devhandl.release_interface(2);
				},
			}
			
		},
		Err(e) => {
			println!(" FAILURED!");
			panic!("INIT_ERROR_LVL_0: {}", e);
		},
	}
	println!("[LAST CALL] Exiting from driver shell!");
}

fn start_listener<T: UsbContext>(dh: &DeviceHandle<T>, state: &mut AppState) {
	let stopped = Arc::new(AtomicBool::new(false));
	match signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&stopped)) {
		Ok(_)=> {
			println!("Init input-event system...");
			
			let abs_x = AbsInfo { value: 0, minimum: 0, 
											maximum: 4096, fuzz: 0, flat: 0, resolution: 20 };
			let abs_y = AbsInfo { value: 0, minimum: 0, 
											maximum: 4096, fuzz: 0, flat: 0, resolution: 30 };
			let abs_pressure = AbsInfo { value: 0, minimum: 0, 
											maximum: 2048, fuzz: 0, flat: 0, resolution: 0 };

			let device = (UninitDevice::new()).unwrap();
			
			device.set_name("dexp_tablet");
			device.set_bustype(BusType::BUS_USB as u16);
			device.set_vendor_id(VENDOR);
			device.set_product_id(DEVICE);
			//device.set_phys("dexp_tablet");
			device.set_version(0x1);
			
			device.enable_event_type(&EventType::EV_ABS).unwrap();
			device.enable_event_type(&EventType::EV_KEY).unwrap();
			
			device.enable(EventCode::EV_KEY(EV_KEY::BTN_TOOL_PEN)).unwrap();
			device.enable(EventCode::EV_KEY(EV_KEY::BTN_TOUCH)).unwrap();
			
			println!("Registering aliases... ");
			for (key, array) in state.buttons.clone() {
				for bind in array {
					if bind.contains("@asRel_") {
						device.enable_event_type(&EventType::EV_REL).unwrap();
						let target = bind.replace("@asRel_", "");
						let scode: Vec<&str> = target.split("@").collect();
						match EV_REL::from_str(scode[0]) {
							Ok(code) => device.enable(EventCode::EV_REL(code)).unwrap(),
							Err(e) => { println!("Bad alias {} in section {}. Skipping with error {:?}", bind, key, e); }
						}
					}else {
						match EV_KEY::from_str(bind.as_str()) {
							Ok(code) => device.enable(EventCode::EV_KEY(code)).unwrap(),
							Err(e) => { println!("Bad alias {} in section {}. Skipping with error {:?}", bind, key, e); }
						}
					}
				}
			}
			
			device.enable_event_code(&EventCode::EV_ABS(evdev_rs::enums::EV_ABS::ABS_X), 
				Some(EnableCodeData::AbsInfo(abs_x))).unwrap();
			device.enable_event_code(&EventCode::EV_ABS(evdev_rs::enums::EV_ABS::ABS_Y), 
				Some(EnableCodeData::AbsInfo(abs_y))).unwrap();
			device.enable_event_code(&EventCode::EV_ABS(evdev_rs::enums::EV_ABS::ABS_PRESSURE), 
				Some(EnableCodeData::AbsInfo(abs_pressure))).unwrap();
					
			device.enable(EventCode::EV_SYN(EV_SYN::SYN_REPORT)).unwrap();
			device.enable(EventCode::EV_SYN(EV_SYN::SYN_DROPPED)).unwrap();
			
			let tablet = UInputDevice::create_from_device(&device).expect("FAILURED!");
			println!("Working with device {:?}, starting listening...", tablet.syspath().unwrap());		
			
			let mut data: [u8; 64] = [0; 64];
			let mut refresh_time = 10;
			while !stopped.load(Ordering::Relaxed) {
				sleep(Duration::from_millis(refresh_time));
				match dh.read_interrupt(0x83, &mut data, Duration::from_millis(250)) {
					Err(_) => { refresh_time = 25 },
					Ok(_) => {
						refresh_time = 7;
						println!("ARRAY OF {:?}", data);
						
						let mut x_coord: i32 = {
							let xtmp = data[1] as i32 *255+data[2] as i32; //+data[13] as i16;
							if state.inv_x { 4096-xtmp }else { xtmp }
						};
						let mut y_coord: i32 = {
							let ytmp = data[3] as i32 *255+data[4] as i32;// +data[14] as i16 ;
							if state.inv_y { 4096-ytmp } else { ytmp }
						};
						
						if state.swap {
							let chain = x_coord;
							x_coord = y_coord;
							y_coord = chain;
						}
						
						let pressure = calc_pressure(data[5], data[6], state);
						println!("X_COORD: {}, Y_COORD: {}, SENSOR_PRESSURE: {}", &x_coord, &y_coord, &pressure);
						
						let time = std::time::SystemTime::now()
								.duration_since(std::time::UNIX_EPOCH).unwrap();
						
						tablet.write_event(&InputEvent {
							  time: TimeVal::new(time.as_secs() as i64, time.subsec_millis() as i64),
							  event_code: EventCode::EV_ABS(evdev_rs::enums::EV_ABS::ABS_X),
							  value: x_coord,
							}
						).expect("FAILURED");
						
						tablet.write_event(&InputEvent {
							  time: TimeVal::new(time.as_secs() as i64, time.subsec_millis() as i64),
							  event_code: EventCode::EV_ABS(evdev_rs::enums::EV_ABS::ABS_Y),
							  value: y_coord,
							}
						).expect("FAILURED");
						
						tablet.write_event(&InputEvent {
							  time: TimeVal::new(time.as_secs() as i64, time.subsec_millis() as i64),
							  event_code: EventCode::EV_ABS(evdev_rs::enums::EV_ABS::ABS_PRESSURE),
							  value: pressure as i32,
							}
						).expect("FAILURED");
						
						let _ = tablet.write_event(&InputEvent {
							  time: TimeVal::new(time.as_secs() as i64, time.subsec_millis() as i64),
							  event_code: EventCode::EV_SYN(EV_SYN::SYN_REPORT),
							  value: 0,
							}
						);
						match data[11] {
							255 => {
								if state.button_clicks[1] != 0 {
									button_click(&tablet, &state, 0, "KEY_L2", &time);
									state.button_clicks[1] = 0;	
								}
								if state.button_clicks[2] != 0 {
									button_click(&tablet, &state, 0, "KEY_L3", &time);
									state.button_clicks[2] = 0;	
								}
								if state.button_clicks[3] != 0 {
									button_click(&tablet, &state, 0, "KEY_L4", &time);
									state.button_clicks[3] = 0;	
								}
								if state.button_clicks[4] != 0 {
									button_click(&tablet, &state, 0, "KEY_L5", &time);
									state.button_clicks[4] = 0;	
								}
								if state.button_clicks[5] != 0 {
									button_click(&tablet, &state, 0, "KEY_L6", &time);
									state.button_clicks[5] = 0;	
								}
								
								if state.button_clicks[9] != 0 {
									button_click(&tablet, &state, 0, "KEY_R4", &time);
									state.button_clicks[9] = 0;	
								}
								if state.button_clicks[10] != 0 {
									button_click(&tablet, &state, 0, "KEY_R5", &time);
									state.button_clicks[10] = 0;	
								}
								if state.button_clicks[11] != 0 {
									button_click(&tablet, &state, 0, "KEY_R6", &time);
									state.button_clicks[11] = 0;	
								}
							},
							127 => {
								button_click(&tablet, &state, 1, "KEY_L2", &time);
								state.button_clicks[1] = 1;
							},
							191 => {
								button_click(&tablet, &state, 1, "KEY_L3", &time);
								state.button_clicks[2] = 1;
							},
							223 => {
								button_click(&tablet, &state, 1, "KEY_L4", &time);
								state.button_clicks[3] = 1;
							},
							239 => {
								button_click(&tablet, &state, 1, "KEY_L5", &time);
								state.button_clicks[4] = 1;
							},
							247 => {
								button_click(&tablet, &state, 1, "KEY_L6", &time);
								state.button_clicks[5] = 1;
							},
							254 => {
								button_click(&tablet, &state, 1, "KEY_R4", &time);
								state.button_clicks[9] = 1;
							},
							253 => {
								button_click(&tablet, &state, 1, "KEY_R5", &time);
								state.button_clicks[10] = 1;
							},
							251 => {
								button_click(&tablet, &state, 1, "KEY_R6", &time);
								state.button_clicks[11] = 1;
							},
							_=> {}
						}
						match data[12] {
							51 => {
								if state.button_clicks[0] != 0 {
									button_click(&tablet, &state, 0, "KEY_L1", &time);
									state.button_clicks[0] = 0;	
								}
								if state.button_clicks[6] != 0 {
									button_click(&tablet, &state, 0, "KEY_R1", &time);
									state.button_clicks[6] = 0;	
								}
								if state.button_clicks[7] != 0 {
									button_click(&tablet, &state, 0, "KEY_R2", &time);
									state.button_clicks[7] = 0;	
								}
								if state.button_clicks[8] != 0 {
									button_click(&tablet, &state, 0, "KEY_R3", &time);
									state.button_clicks[8] = 0;	
								}
							},
							49 => {
								button_click(&tablet, &state, 1, "KEY_L1", &time);
								state.button_clicks[0] = 1;
							}
							35 => {
								button_click(&tablet, &state, 1, "KEY_R1", &time);
								state.button_clicks[6] = 1;
							},
							50 => {
								button_click(&tablet, &state, 1, "KEY_R2", &time);
								state.button_clicks[7] = 1;
							},
							19 => {
								button_click(&tablet, &state, 1, "KEY_R3", &time);
								state.button_clicks[8] = 1;
							},
							_ =>{}
						}
						
						match data[9] {
							2 => {
								if state.button_clicks[12] != 0 {
									button_click(&tablet, &state, 0, "VPEN_PLUS", &time);
									state.button_clicks[12] = 0;
								}
								if state.button_clicks[13] != 0 {
									button_click(&tablet, &state, 0, "VPEN_MINUS", &time);
									state.button_clicks[13] = 0;
								}
							},
							4 => {
								button_click(&tablet, &state, 1, "VPEN_PLUS", &time);
								state.button_clicks[12] = 1;
							},
							6 => {
								button_click(&tablet, &state, 1, "VPEN_MINUS", &time);
								state.button_clicks[13] = 1;
							},
							_ => {}
						}
						//println!("DATA OF {:?}", state.button_clicks);
						
						let val: i32 = match data[5] {
							6=>0, _=>1
						};
						
						tablet.write_event(&InputEvent {
							  time: TimeVal::new(time.as_secs() as i64, time.subsec_millis() as i64),
							  event_code: EventCode::EV_KEY(evdev_rs::enums::EV_KEY::BTN_TOUCH),
							  value: val,
							}
						).expect("FAILURED");
						let _ = tablet.write_event(&InputEvent {
							  time: TimeVal::new(time.as_secs() as i64, time.subsec_millis() as i64),
							  event_code: EventCode::EV_SYN(EV_SYN::SYN_REPORT),
							  value: 0,
							}
						);
					}
				};
			}
		},
		Err(err) => panic!("Cannot init main_loop. Stopped with error: {}", err)
	};
	println!("Exiting from loop...");
}

fn button_click(device: &UInputDevice, state: &AppState, clicked: i32, button: &str, time: &Duration) {
	match state.buttons.get(button) {
		Some(obj)=>	{
			//println!("Clicking of {:?} with state {}", &obj, &clicked);
			for binder in obj {
				if binder.contains("@asRel_") {
					let target = binder.replace("@asRel_", "");
					let scode: Vec<&str> = target.split("@").collect();
					match EV_REL::from_str(scode[0]) {
						Ok(code) => {
							let val: i32 = {
								if scode[1].eq("ADD") { 1 }
								else if scode[1].eq("REM") { -1 }
								else { 0 }
							};
							let _ = device.write_event(&InputEvent {
								  time: TimeVal::new(time.as_secs() as i64, time.subsec_millis() as i64),
								  event_code: EventCode::EV_REL(code),
								  value: val,
								}
							);
							let _ = device.write_event(&InputEvent {
								  time: TimeVal::new(time.as_secs() as i64, time.subsec_millis() as i64),
								  event_code: EventCode::EV_SYN(EV_SYN::SYN_REPORT),
								  value: 0,
								}
							);
						},
						Err(e) => { println!("Bad WHEEL {} in section. Skipping with error {:?}", binder, e); }
					}
				}else {
					match EV_KEY::from_str(binder) {
						Ok(alias)=> {
							let _ = device.write_event(&InputEvent {
								  time: TimeVal::new(time.as_secs() as i64, time.subsec_millis() as i64),
								  event_code: EventCode::EV_KEY(alias),
								  value: clicked,
								}
							);
						},
						Err(e)=>{ println!("Bad KEY alias {}, skip with error {:?}", binder, e); }
					}
				}
			}
			let _ = device.write_event(&InputEvent {
				  time: TimeVal::new(time.as_secs() as i64, time.subsec_millis() as i64),
				  event_code: EventCode::EV_SYN(EV_SYN::SYN_REPORT),
				  value: 0,
				}
			);
		},
		_ => { println!("Key [{}] isn't found in bindings, ignore this", button); }
	}
}


fn calc_pressure(deep: u8, value: u8, state: &AppState) -> f32 {
	let real_deep: f32 = (deep as f32 - 6.).abs();
	let chain: f32 = ((real_deep*255.0+value as f32)*MAX_PRESSURE/1024.0)-state.sensivity as f32;
	if chain < 0.0 { 0. } else { chain }
}

fn enter_advance_mode<T: UsbContext>(dh: &DeviceHandle<T>) {
	print!("Trying to switch advanced mode...");
	let _ = dh.write_control(0x21, 9, 0x0202, 2,
					 &[0x02, 0x00],Duration::from_millis(250));
	let _ = dh.write_control(0x21, 9, 0x0308, 2,
		 	&[0x08, 0x03, 0x00, 0xff, 0xf0, 0x00, 0xff, 0xf0],Duration::from_millis(250));
	let _ = dh.write_control(0x21, 9, 0x0308, 2,
			 &[0x08, 0x07, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff],Duration::from_millis(250));
	let _ = dh.write_control(0x21, 9, 0x0308, 2,
			 &[0x08, 0x03, 0x00, 0xff, 0xf0, 0x00, 0xff, 0xf0],Duration::from_millis(250));
	let _ = dh.write_control(0x21, 9, 0x0308, 2,
			 &[0x08, 0x06, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00],Duration::from_millis(250));
	println!(" Ok!");
}


fn detach_kernel_support<T: UsbContext>(dh: &DeviceHandle<T>) {
	if rusb::supports_detach_kernel_driver() {
		for e in 0..=2 {
			match dh.kernel_driver_active(e) {
				Ok(is_support) => 
					if is_support {
						print!("Trying disable KERNEL_SUPPORT_USB_REVISION_{}", e);
						let _ = dh.detach_kernel_driver(e);
						println!(" Ok!");
				},
				Err(err) => println!("Unsupported detaching for USB_REVISION_{} failured with error {}", e, err)
			}
		}
		//dh.reset();
	} else {
		println!("Error: Detaching isn't support!");
	}
	
	
}

fn catch_tablet<T: UsbContext>(sys: &mut T, vid: u16, pid: u16)
-> Option<(Device<T>, DeviceDescriptor, DeviceHandle<T>)> {
	let devices = match sys.devices() {
		Ok(d) => d,
		Err(_) => return None,
	};
	for d in devices.iter() {
		let devdesc = match d.device_descriptor() {
			Err(_) => continue,
			Ok(value) => value
		};
		if devdesc.vendor_id()==vid && devdesc.product_id() == pid {
			match d.open() {
				Ok(devhandler) => { return Some((d, devdesc, devhandler)); },
				Err(e) => {
					println!(" FAILURED!");
					panic!("Device is unreachable: {}", e);
				}
			}
		}
	}
	None
}

/*
fn main() {
    println!("Welcome to DEXP Tablet Ombra M Driver Shell");
	let word = "ASS_DOTODING".to_string();
	say(&word);
	say(&word);
	say(&word);
	say(&word);
	say(&word);
	
	let mut vector: Vec<usize> = vec![0usize, 4usize, 5usize];
	println!("MEM POOL : {:?}", &vector);
	mutableVec(&mut vector);
	println!("MEM POOL : {:?}", &vector);
//	sleep(Duration::from_millis(2000000));
}

fn mutableVec(array : &mut Vec<usize>) {
    array.push(10);
	array.push(90)
}

fn say(word: &String) {
	println!("Doctor sayed {}", *word);
}*/
	/*
	let system = Context::new();
	
	
	
	match Context::new() {
		Ok(mut context) => match openDevice(&mut context,
											VENDOR, DEVIVE) {
			Some(mut device) => readDevice(),
			None=> println!("Device not found with VID:PID [{:04x}:{:04x}]", VENDOR, DEVICE)
		},
		Err(rusb::e) => {
			panic!("Unresolved error: {}!", e);
		}
	}
}

fn openDevice<T: UsbContext> (system: &UsbContext, vid: u16, pid: u16) {
	
}

fn readDevice() {
	
}*/
