# Proof of concept CoAP proxy

This proxy receives the "pings" from the gateway, queries it's status and fowards this status to the backend server.

The standard way to do CoAP is the constrained device (here the "gateway") acts as a server.
Ariel OS implements everything that's needed to operate a CoAP server, with only a few options to do CoAP requests.
This means we need this proxy to establish a secure connection with the server on the gateway, in a normal network setup we can't direclty reach the gateway's server because it's UDP port is not exposed to the internet.
The solution is to have the gateway send a plain CoAP request to the Proxy first, this will make the routers configure NAT mappings so the gateway can reach the proxy. We can then use the same mappings in reverse, using exactly the same UDP port configuration on the proxy and gateway to send a request from the proxy to the gateway.

Currently the gateway checks the authenticity of the proxy but the proxy does not check the authenticity of the gateway.

## Installation

pipx is needed to manage dependencies of the script. Follow the [installation guide](https://pipx.pypa.io/stable/installation/) for your platform.

On Debian/Ubuntu you can do:

```sh
sudo apt update
sudo apt install pipx
pipx ensurepath
```

## Configuration

### Generate a key

You need to generate a keypair for setting up the secure connection between the proxy and the gateway. Use the provided script:

```sh
pipx run generate.py
```

The command will output a public key, put this key for `kccs` in `../gateway/peers.yml`.

If you want to regenerate a keypair, you first need to delete `server.cosekey`.

### Authentification to the backend

Copy the `.env.example` file to `.env` and complete it with the information (token and url) to push data to the backend.

### Port

By default the proxy listens on UDP port 5683, you can change it using the PORT env variable.

The gateway needs to connect to it through the internet, so this port needs to be open to the internet.

## Run the proxy

After setting the configuration, you can run the proxy using:

```sh
pipx run server.py
```
