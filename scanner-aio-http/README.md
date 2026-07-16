# Scanner "all in one" 

This scans for BLE devices with the 3 bytes address prefix defined with the env variable `TAG_PREFIX` (format XX:XX:XX where X is a hex character) and send each advertisement received that matches the custom data and address prefix with an HTTP POST request to the url defined in `BACKEND_ENDPOINT`.

You can set the sniffer id with the env variable `SNIFFER_ID`
