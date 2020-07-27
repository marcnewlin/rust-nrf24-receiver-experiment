use num::complex::Complex;
use std::fs::File;
use std::io::Read;

fn main() {

    // path to input IQ (10 seconds @ 4MHz sample rate centered at 2460MHz)
    // - recorded with the following command on a USRP B210:
    //   $ uhd_rx_cfile -A TX/RX -r 4e6 -f 2460e6 -g 30 -N 40e6 nrf24-2460-4e6.iq
    let iq_path = "iq/nrf24-2460-4e6.iq";

    // read the IQ file into a byte vector
    let mut file = File::open(iq_path).expect("Unable to open IQ file.");
    let meta = std::fs::metadata(iq_path).expect("Unable to read IQ file metadata.");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("Unable to read IQ file.");

    // convert the bytes to complex floats
    let mut samples: Vec<Complex<f32>> = vec![Default::default(); (meta.len() / 8) as usize];
    let mut real_bytes: [u8; 4] = [0; 4];
    let mut imag_bytes: [u8; 4] = [0; 4];
    for i in 0..samples.len() {
        real_bytes.copy_from_slice(&bytes[i*8+0..i*8+4]);
        imag_bytes.copy_from_slice(&bytes[i*8+4..i*8+8]);
        samples[i].re = f32::from_le_bytes(real_bytes);
        samples[i].im = f32::from_le_bytes(imag_bytes);
    }

    // demodulate the input IQ (2SPS) to bits
    let bits = bpsk_demod(&mut samples, 2.0);

    // look for packets in the demodulated bitstream
    let mut alt_count = 0;
    for i in 1..(bits.len() - 64*8) {

        // update the alternating bit count 
        if bits[i] != bits[i-1] {
            alt_count += 1;
        } else {
            alt_count = 0;
        }

        // check for a possible preamble + first address bit
        if alt_count >= 9 && alt_count <= 17 {

            // parse address (assumes 4-byte address)
            let mut address: [u8; 4] = [0; 4];
            let mut offset = i;
            for ibyte in 0..4 {
                for ibit in 0..8 {
                    address[ibyte] <<= 1;
                    address[ibyte] |= bits[offset+ibyte*8+ibit];
                }
            }
            offset += 32;

            // parse packet length (6 bits)
            let mut length: u8 = 0;
            for ibit in 0..6 {
                length <<= 1;
                length |= bits[offset+ibit];
            }
            offset += 6;

            // filter out invalid lengths
            if length > 32 {
                continue;
            }

            // parse packet ID (2 bits)
            let pid: u8 = bits[offset]<<1 | bits[offset+1];
            offset += 2;

            // parse no-ACK bit
            // let no_ack: u8 = bits[offset];
            offset += 1;

            // parse payload
            let mut payload: Vec<u8> = vec![0; length as usize];
            for ibyte in 0..length {
                for _ibit in 0..8 {
                    payload[ibyte as usize] <<= 1;
                    payload[ibyte as usize] |= bits[offset];
                    offset += 1;
                }
            }
            
            // parse CRC
            let mut crc_given: u16 = 0;
            for _ibyte in 0..2 {
                for _ibit in 0..8 {
                    crc_given <<= 1;
                    crc_given |= bits[offset] as u16;
                    offset += 1;
                }
            }

            // compute CRC
            let total_bits = 32 /* address */ + 9 /* PCF */ + length*8;
            let mut crc_calc: u16 = 0xffff;
            for ibit in 0..total_bits {
                if bits[i+ibit as usize] != ((crc_calc >> 15) as u8) {
                    crc_calc = (crc_calc << 1) ^ 0x1021;
                }
                else {
                    crc_calc <<= 1;
                }
            }

            // check CRC
            if crc_calc == crc_given {

                // print the address to stdout
                print!("address=");
                for ibyte in 0..4 {
                    print!("{:02x}", address[ibyte]);
                }
                print!(",  ");

                // print the PID to stdout
                print!("pld={},  ", pid);

                // print the payload to stdout
                print!("payload=");
                for ibyte in 0..length {               
                    print!("{:02x}", payload[ibyte as usize]);
                }
                print!("\n");   
            }         
        }
    }
}

fn slice(val: f32) -> f32 {
    if val < 0.0 {
        return -1.0;
    } 
    else {
        return 1.0;
    }
}

fn sinc(x: f32) -> f32 {
    if x == 0.0 {
        return 1.0;
    }
    let pi_x = x * 3.14159;
    return pi_x.sin() / pi_x;
}

fn bpsk_demod(samples: &mut Vec<Complex<f32>>, sps: f32) -> Vec<u8> {

    // quadrature demodulate
    let mut soft_demod: Vec<f32> = vec![0.0; samples.len()-1];
    for i in 1..samples.len() {
        let s = samples[i].conj() * samples[i-1];
        soft_demod[i-1] = s.arg();
    }

    // generate sync filter taps for interpolator
    let mut taps: [f32; 1032] = [0.0; 1032];
    let mut offset = 0.0;
    let step = 0.25 / 129.0;
    for i in 0..129 {
        for itap in 0..8 {
            taps[i*8+itap] = sinc(-4.0 + (itap as f32) + offset)
        }
        offset += step;
    }

    // clock recovery parameters and state
    let mut sps_actual = sps;
    let sps_expected = sps;
    let sps_tolerance = 0.005;    
    let gain_sample_offset = 0.175;
    let gain_sps = 0.25 * gain_sample_offset * gain_sample_offset;    
    let mut sample_offset = 0.5;
    let mut last_sample = 0.0;

    // perform clock recovery
    for i in 0..soft_demod.len() {

        // compute the interpolator filter coefficient offset
        let filter_offset : usize = (((sample_offset * 129.0) as i32) * 8) as usize;

        // interpolate the output sample
        if i >= 8 {

            // compute the dot product
            let mut out : f32 = 0.0;
            for si in 0..8 {
                out += taps[filter_offset] * soft_demod[i-si];
            }
            soft_demod[i] = out;
        }

        // calculate the error value (Muller & Mueller)
        let error = slice(last_sample) * soft_demod[i] - slice(soft_demod[i]) * last_sample;
        last_sample = soft_demod[i];

        // update the actual samples per symbol
        sps_actual = sps_actual + gain_sps * error;
        sps_actual = sps_actual.min(sps_expected+sps_tolerance).max(sps_expected-sps_tolerance);

        // update the fractional sample offset
        sample_offset = sample_offset + sps_actual + gain_sample_offset * error;
        sample_offset = sample_offset - sample_offset.floor();
    }

    // slice the bits
    let mut bits: Vec<u8> = vec![Default::default(); samples.len()/2-1];
    for i in (2..soft_demod.len()).step_by(2) {
        if soft_demod[i] < 0.0 { bits[i/2-1] = 0; }
        else { bits[i/2-1] = 1; }
    }

    return bits;
}
