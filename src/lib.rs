// E32-TTL-100 Lora radio driver


#![crate_type = "lib"]
#![crate_name = "lora_driver"]

// STD includes
use std::io::Write;
use std::io::Read;
use std::io::{Error, ErrorKind};
use std::string::String;
use std::thread::sleep;
use std::time::Duration;
use std::env;
// sysfs_gpio crate includes
extern crate sysfs_gpio;
use sysfs_gpio::{Direction, Edge, Pin};
// bit_vec crate includes
extern crate bit_vec;
use bit_vec::BitVec;
// serial crate includes
extern crate serial;
use serial::SerialDevice;
use serial::SerialPortSettings;

// Let's begin our definitions of the objects we'll use to control the LORA radios

#[derive(Copy, Clone)]
pub struct RadioConfig {
	head: u8,
	addh: u8,
	addl: u8,
	sped: u8,
	chan: u8,
	option: u8
}

// Initially only the options to change important configuration data
// will be exposed, things like serial parity bit and module address high byte
// will be left out for now/until they're needed
impl RadioConfig {

	// create a new config with the default values from the doc
	pub fn new() -> RadioConfig {
		RadioConfig{ head: 192,	//C0x
					 addh: 18,	//12x
					 addl: 52,	//34x
					 sped: 24,	//18x
					 chan: 80,	//50x
					 option: 64	//40x
		}
	}

	// This function outputs the actual binary data that needs to be sent out the UART connection.
	pub fn raw(&self) -> [u8;6] {
		[self.head,self.addh,self.addl,self.sped,self.chan,self.option]
	}

	pub fn set_address(&mut self, new_address: &str) {
		//change the addh and addl off of the new string
		let new_addrh_str = &new_address[..2]; // Take first 2 bytes
		let new_addrl_str = &new_address[2..4];	// Take last 2 bytes
		//Convert these base16 numbers to base10
		let new_addrh = u8::from_str_radix(new_addrh_str, 16).unwrap();
		let new_addrl = u8::from_str_radix(new_addrl_str, 16).unwrap();
		self.addh = new_addrh;
		self.addl = new_addrl;
	}

	pub fn set_serial_rate(&mut self, ser_speed: &str) {
		// These bits are where the serial rate config option lives.
		let config_bits_offsets = [5,4,3];
		let config_bits_values;
		// Needs to be matched as reference because rust
		match ser_speed {
			"1200" => {config_bits_values = [0,0,0]}, // bits alre already all 0, do nothing
			"2400" => {config_bits_values = [0,0,1]},
			"4800" => {config_bits_values = [0,1,0]},
			"9600" => {config_bits_values = [0,1,1]},
			"19200" => {config_bits_values = [1,0,0]},
			"38400" => {config_bits_values = [1,0,1]},
			"57600" => {config_bits_values = [1,1,0]},
			"115200" => {config_bits_values = [1,1,1]},
			_ => panic!("Incorrect serial speed specified, halting")
		}

		// Save our changes in settings back to the RadioConfig struct
		self.sped = self.change_bits(self.sped,&config_bits_offsets,&config_bits_values);
	}

	pub fn set_air_rate(&mut self, air_speed: &str) {
		// These bits are where the air rate config option lives.
		let config_bits_offsets = [2,1,0];
		let config_bits_values;
		// Needs to be matched as reference because rust
		match air_speed {
			"1k" => {config_bits_values = [0,0,0]}, // bits alre already all 0, do nothing
			"2k" => {config_bits_values = [0,0,1]},
			"5k" => {config_bits_values = [0,1,0]},
			"10k" => {config_bits_values = [0,1,1]},
			"12k" => {config_bits_values = [1,0,0]},
			"15k" => {config_bits_values = [1,0,1]},
			"20k" => {config_bits_values = [1,1,0]},
			"25k" => {config_bits_values = [1,1,1]},
			_ => panic!("Incorrect air speed specified, halting")
		}

		// Save our changes in settings back to the RadioConfig struct
		self.sped = self.change_bits(self.sped,&config_bits_offsets,&config_bits_values);
	}

	pub fn set_transmit_power(&mut self, transmit_power: &str) {
		// These bits are where the air rate config option lives.
		let config_bits_offsets = [2,1,0];
		let config_bits_values;
		// Needs to be matched as reference because rust
		match transmit_power {
			"20dBm" => {config_bits_values = [0,0,0]}, // bits alre already all 0, do nothing
			"17dBm" => {config_bits_values = [0,0,1]},
			"14dBm" => {config_bits_values = [0,1,0]},
			"11dBm" => {config_bits_values = [0,1,1]},
			"8dBm" => {config_bits_values = [1,0,0]},
			"5dBm" => {config_bits_values = [1,0,1]},
			"2.5dBm" => {config_bits_values = [1,1,0]},
			"0dBm" => {config_bits_values = [1,1,1]},
			_ => panic!("Incorrect transmit power specified, halting")
		}

		// Save our changes in settings back to the RadioConfig struct
		self.option = self.change_bits(self.option,&config_bits_offsets,&config_bits_values);
	}

	// original_byte - the command byte we want to modify
	// target_bits - the bits we want to modify, e.g. [7,6,5]
	// bits to write - the result we want, e.g. [false, true, true]
	fn change_bits(&self, original_byte: u8, target_bits: &[i8], bits_to_write: &[u8]) -> u8 {
		// DOC: 	[76543210]
		// RUST:	[01234567]
		let mut bv = BitVec::from_bytes(&[original_byte]);
		let mut bits_to_write_vec = BitVec::from_elem(bits_to_write.len(),false);
		// convert our passed arg to a bitvec
		for i in 0..bits_to_write.len() {
			match bits_to_write[i] {
				0 => bits_to_write_vec.set(i,false),
				1 => bits_to_write_vec.set(i,true),
				_ => panic!("Invalid bits passed to change_bits, halting")
			}
		}

		// make a local copy of the target bits
		let mut target_bits = target_bits.to_vec();
		// flip our target bits "endian-ness"
		for i in 0..target_bits.len() {
			target_bits[i] = (target_bits[i]-7).abs();
		}

		for i in 0..target_bits.len() {
			bv.set((target_bits[i] as usize),bits_to_write_vec[i]);
		}

		bv.to_bytes()[0]
	}
}

// Constants for control of our radio modules
#[allow(dead_code)]
#[derive(Clone)]
#[derive(Debug)]
pub enum RadioMode {
	General,
	Wakeup,
	PowerSaving,
	Sleep
}

#[allow(dead_code)]
pub struct Driver {
	// Control pin definitions
	m0: Pin,
	m1: Pin,
	aux: Pin,
	mode: RadioMode,
	tty_device_string: String,
	tty_device: serial::SystemPort
}

// Driver functions
#[allow(dead_code)]
impl Driver {

	// Driver should be instantiated with the 3 main pins to control the radio. M0, M1, and AUX.
	pub fn new(m0_pin_num: u64, m1_pin_num: u64, aux_pin_num: u64, tty_str: &str) -> Driver {
		let m0_pin = Pin::new(m0_pin_num);
		let m1_pin = Pin::new(m1_pin_num);
		let aux_pin = Pin::new(aux_pin_num);

		//TODO: Find a better way to do error handling
		match m0_pin.set_direction(Direction::Out) {
			Ok(()) => println!("M0 set correctly"),
			Err(e) => panic!("{}, correct gpio pin?",e)
		}
		match m1_pin.set_direction(Direction::Out) {
			Ok(()) => println!("M1 set correctly"),
			Err(e) => println!("{}, correct gpio pin?",e)
		}
		match aux_pin.set_direction(Direction::In) {
			Ok(()) => println!("AUX set correctly"),
			Err(e) => println!("{}, correct gpio pin?",e)
		}

		let mut port = serial::open(tty_str).unwrap();

		port.set_timeout(Duration::from_millis(100)).unwrap();

		Driver{m0: Pin::new(m0_pin_num), 
			   m1: Pin::new(m1_pin_num), 
			   aux: Pin::new(aux_pin_num),
			   mode: RadioMode::Sleep,
			   tty_device_string: String::from(tty_str),
			   tty_device: port
			}
	}

	fn wait_for_aux(&mut self, value: u8, timeout: u32) -> Result<(), std::io::Error> {
		// Before we eat up any cycles setting up our pollers, check to make sure aux isn't already desired value
		if self.aux.get_value().unwrap() == value {
			// We need an early return here because it isn't the final expression in this function
			return Ok(())
		}

		let mut poller = self.aux.get_poller().unwrap();

		if value == 1 {
			// We're waiting for a rising edge
			self.aux.set_edge(Edge::RisingEdge).expect("Edge failed to set to rising");
			//If the pin is already high by the time we get here there will be an error
			match poller.poll(timeout as isize).unwrap() {
				// Return nothing, we got the result we were looking for.
				Some(value) => { Ok(()) } 
				None => { 
					// Do a final check to make sure our value is incorrect and we didn't just miss the edge
					let value = self.aux.get_value().unwrap();
					if value != 1 {
						Err(Error::new(ErrorKind::TimedOut,"Interrupt timed out"))
					}
					else {
						Ok(())
					}
				}
			}
		}
		else {
			// We're detecting a 0, so we're waiting for a falling edge
			self.aux.set_edge(Edge::FallingEdge).expect("Edge failed to set to falling");
			//If the pin is already low by the time we get here there will be an error
			match poller.poll(timeout as isize).unwrap() {
				Some(value) => { Ok(()) } //println!("Aux low: {}",value),
				None => { 
					// Do a final check to make sure our value is incorrect and we didn't just miss the edge
					let value = self.aux.get_value().unwrap();
					if value != 0 {
						Err(Error::new(ErrorKind::TimedOut,"Interrupt timed out"))
					}
					else {
						Ok(())
					}
				}
			}
		}
	}

	pub fn get_control_gpio_pins(&self) -> (u64, u64, u64) {
		(self.m0.get_pin_num(), self.m1.get_pin_num(), self.aux.get_pin_num())
	}

	// This function will simply panic if there is any error, because it isn't something we can continue operating with.
	pub fn set_mode(&mut self, mode: RadioMode) {

		self.wait_for_aux(1,1000).unwrap();

		
		match mode {
			RadioMode::General => { self.m0.set_value(0).unwrap(); self.m1.set_value(0).unwrap() },
			RadioMode::Wakeup => { self.m0.set_value(0).unwrap(); self.m1.set_value(1).unwrap() },
			RadioMode::PowerSaving => { self.m0.set_value(1).unwrap(); self.m1.set_value(0).unwrap() },
			RadioMode::Sleep => { self.m0.set_value(1).unwrap(); self.m1.set_value(1).unwrap() },
		}

		self.wait_for_aux(1,1000).unwrap();
		// Wait at least 2 ms as per the datasheet
		sleep(Duration::from_millis(2));

		// Then set the mode variable in the driver struct
		self.mode = mode;
		//println!("Mode set to {:?}", self.mode);
	}

	// Get a reference to the driver's mode
	pub fn get_mode(&self) -> &RadioMode {
		&self.mode
	}

	
	pub fn set_tty_baud(&mut self, new_baud: serial::BaudRate) {
		let mut tty_settings = self.tty_device.read_settings().unwrap();
		tty_settings.set_baud_rate(new_baud).unwrap();
		// Set the new baud rate.
		self.tty_device.write_settings(&tty_settings).unwrap();
	}

	pub fn get_tty_baud(&self) -> serial::BaudRate {
		let tty_settings = self.tty_device.read_settings().unwrap();
		tty_settings.baud_rate().unwrap()
	}
	

	pub fn write_config(&mut self, config: RadioConfig) {

		// Declare our read buffer
		//let mut read_buf: Vec<u8> = (0..6).collect();

		// Save the previous mode
		let prev_mode = self.mode.clone();
		self.set_mode(RadioMode::Sleep);

		// Wait at least 2 ms as per the datasheet
		sleep(Duration::from_millis(2));

		// We need to set the linux tty mode to 9200 baud, first saving the old
		//let mut tty_settings = self.tty_device.read_settings().unwrap();
		let orig_baud = self.get_tty_baud();
		// Set the new baud rate.
		self.set_tty_baud(serial::Baud9600);


		// that is the speed that the device operates at in sleep mode
		self.serial_write(config.raw().as_ref());
		// verify what it returns before continuing
		let bytes_read = self.serial_read();
		//println!("Config: read {} bytes in response", bytes_read.len());
		// Return to the mode we were in previously
		self.set_mode(prev_mode);


		//bytes_read.truncate(6); // Truncate our bytes read to the first 6, because those are all we're interested in.
		// If the configs aren't the same, something went wrong and we need to quit
		if bytes_read != config.raw() {
			panic!("Config wasn't written successfully! {:?} vs {:?}",bytes_read,config.raw());
		}
		println!("Config written successfully {:?}",bytes_read);

		//Return the device baud rate to the original
		self.set_tty_baud(orig_baud);
	}

	fn serial_write(&mut self, data: &[u8]) {

		let bytes_wrote = self.tty_device.write(data).unwrap();
		//println!("Wrote {} bytes", bytes_wrote);

	}

	// TODO: look into buffered reader
	// Allow unused variables for the error catching we're expecting.
	#[allow(unused_variables)]
	pub fn serial_read(&mut self) -> Vec<u8>{

		// Create the exact error we're looking for.
		let expected_error = Error::new(ErrorKind::TimedOut,"Operation timed out");
		// Create a vect that contains max capacity of the packet
		let mut buf_vec: Vec<u8> = Vec::with_capacity(256);
		let mut loop_buf: Vec<u8> = vec![0;58]; // 58 is the actual max packet size of the radios.
		let mut bytes_read: usize = 0;
		loop {
			match self.tty_device.read(&mut loop_buf) {
				Ok(size) => { 
					// Add to the buffer and increment the amount read
					let mut tmp_buf = loop_buf.clone();
					tmp_buf.truncate(size);
					buf_vec.append(&mut tmp_buf);
					bytes_read += size;
				},
				Err(err) => {
					// Use raw os error because for some reason std::io:Error doesn't implement PartialEq
					if err.raw_os_error() == expected_error.raw_os_error() {
						break;
					}
					else {
						panic!(err);
					}
				}
			}
		}
		buf_vec.shrink_to_fit();
		buf_vec
	}

	pub fn set_tty_params(&mut self, br: serial::BaudRate, 
									 cs: serial::CharSize,
									 p: serial::Parity,
									 sb: serial::StopBits,
									 fc: serial::FlowControl ) {
									 
		let mut settings = self.tty_device.read_settings().unwrap();
		settings.set_baud_rate(br).unwrap();
		settings.set_char_size(cs);
		settings.set_parity(p);
		settings.set_stop_bits(sb);
		settings.set_flow_control(fc);

		self.tty_device.write_settings(&settings).unwrap();
	}

	pub fn send_packet(&mut self, packet: &Vec<u8>, timeout: u32) -> Result<(), std::io::Error> {
		
		// Make sure our packet isn't larger than is allowed by the device.
		if packet.len() > 58 {
			return Err(Error::new(ErrorKind::InvalidInput,"Attempted to send a packet that was too long"));
		}

		// make sure the pin is high before we start sending
		self.wait_for_aux(1,timeout)?; //Aux never came high! Cannot send
		// Send the packet!
		self.serial_write(packet);
		self.wait_for_aux(1,5000).expect("Aux never came back up, stuck low after sending"); //I'm not sure how long it will take the radio to send all the data. Let's use 1 second for now
		//println!("Sent {} bytes of data!",packet.len());
		Ok(())
	}

	pub fn receive_packet(&mut self, timeout_ms: u32) -> Result<Vec<u8>, std::io::Error> {
		// Make sure aux is high, meaning we can do something
		self.wait_for_aux(1,5000).expect("Aux stuck low... radio is down or very busy");
		// Wait for aux to be low, that signifies that the radio is getting data
		// We also only want to propegate this timeout error
		self.wait_for_aux(0,timeout_ms)?;
		// Wait at least 5ms according to the doc
		//sleep(Duration::from_millis(5));
		// Wait for the radio to finish sending data to the serial port for us to read.
		self.wait_for_aux(1,5000).expect("Radio took too long to write to serial, timed out");

		// Read the data
		// TODO: Make sure all the data is read correctly.
		Ok(self.serial_read())
		//Ok(serial_data)

	}
}