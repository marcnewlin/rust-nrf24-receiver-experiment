# rust-nrf24-receiver-experiment
small experiment to learn some rust via a nRF24 Enhanced Shockburst receiver (2SPS IQ -> packets)

### iq/nrf24-2460-4e6.iq

IQ from a channel with an nRF24 dongle transmitting a repeating payload, captured with a USRP B210 using the following command:

```
uhd_rx_cfile -A TX/RX -r 4e6 -f 2460e6 -g 30 -N 40e6 nrf24-2460-4e6.iq
```

### receiver/src/main.rs

Minimal PSK receiver and Nordic Enhanced Shockburst packet framer.
- input IQ is assumed to be 2SPS
- M&M timing recovery
- simple sinc-filter fractional-delay interpolator
- ESB packets are configured for dynamic payload lengths and 2-byte CRCs

### Usage

```
$ make
$ ./receiver-release
```

Decoded payloads will be printed to standard out.