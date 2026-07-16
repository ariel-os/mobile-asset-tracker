# Scanner "all in one" 

This scans for BLE devices with the 3 bytes address prefix defined with the env variable `TAG_PREFIX` (format XX:XX:XX where X is a hex character) and send each advertisement received that matches the custom data and address prefix with an HTTP POST request to the url defined in `BACKEND_ENDPOINT`.

> Note: since the advertisement code uses a static random address, the first two bits of the address will always be 1s. This means the first hex character of the address must be C,D,E or F.

You can set the sniffer id with the env variable `SNIFFER_ID`.

For devices that use wifi for networking, you also need to set `CONFIG_WIFI_NETWORK` and `CONFIG_WIFI_PASSWORD` as environment variables.

Example for a xiao esp32c6 (wifi):

```sh
CONFIG_WIFI_NETWORK=my_network CONFIG_WIFI_PASSWORD=my_network_password BACKEND_ENDPOINT=192.168.1.16:3000/advertisement TAG_PREFIX=CC::BB::AA SNIFFER_ID=sniffer-1 laze build -b seeedstudio-xiao-esp32c6 run
```
