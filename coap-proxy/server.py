#!/usr/bin/env python3
# /// script
# dependencies = [
#   "aiocoap >= 0.4.11, < 0.5",
#   "cbor2 >= 5.8.0, < 6.0",
#   "cbor-diag >= 1.0.0, < 2.0",
#   "lakers-python >= 0.6.0,",
#   "cryptography >= 46.0.0, < 47.0",
#   "filelock >= 3.24.0, < 4.0",
#   "python-dotenv >= 1.2.0, < 2.0",
#   "aiohttp >= 3.10.0, < 4.0",
# ]
# ///
"""
Minimal server providing a single resource /uppercase, to which ASCII text can
be POSTed; the text is returned IN ALL CAPS.
"""

import asyncio
import logging
from pathlib import Path
from typing import Any
import os
from dotenv import load_dotenv


import json
import aiohttp

import cbor2
import aiocoap
from aiocoap import Message, GET, Context
from aiocoap.resource import Resource, Site
from aiocoap.credentials import CredentialsMap
from aiocoap.oscore_sitewrapper import OscoreSiteWrapper
from aiocoap.numbers.codes import Code

servers: set[str] = set()

load_dotenv()

BACKEND_ENDPOINT = os.getenv("BACKEND_ENDPOINT")
BEARER_TOKEN = os.getenv("BEARER_TOKEN")
PORT = 5683

env_port = os.getenv("PORT")


if env_port is not None:
    PORT = int(env_port)


# minicbor doesn't set names to fields, we have to manually restore them
def convert_gateway_update(cbor: list):
    out: dict[str, Any] = dict()

    out["gatewayId"] = cbor[0]
    out["timestamp"] = cbor[1]
    out["detected_tags"] = convert_detected_tags(cbor[2])
    if len(cbor) >= 4:
        out["batteryLevel"] = cbor[3]
    else:
        out["batteryLevel"] = None
    if len(cbor) >= 5:
        out["location"] = convert_location(cbor[4])
    else:
        out["location"] = None
    return out


def convert_detected_tags(cbor: list):
    out: list[dict[str, Any]] = []

    for tag_cbor in cbor:
        tag_out: dict[str, Any] = dict()
        tag_out["id"] = tag_cbor[0]
        tag_out["age"] = tag_cbor[1]
        tag_out["rssi"] = tag_cbor[2]

        out.append(tag_out)

    return out


def convert_location(cbor: list):
    out: dict[str, float | int] = dict()

    out["latitude"] = cbor[0]
    out["longitude"] = cbor[1]
    out["altitude"] = cbor[2]
    out["heading"] = cbor[3]
    out["horizontalSpeed"] = cbor[4]
    out["verticalSpeed"] = cbor[5]
    out["timeOfFix"] = cbor[6]

    return out


class Register(Resource):
    async def render_post(self, request):

        # This represents the IP and port we can use to contact the device.
        remote = request.remote.uri_base

        print("received ping from device:", remote)
        servers.add(remote)

        try:
            # directly request the update to the remote
            await process_update(remote)
        except Exception as e:
            print("Error when processing update:", e)
            return aiocoap.Message(
                content_format=0,
                payload=b"Internal Server Error",
                code=Code.INTERNAL_SERVER_ERROR,
            )

        return aiocoap.Message(content_format=0, payload=b"OK")


# background_tasks = set()
context = None

server_credentials_file = Path("server.diag")
credentials = dict()

try:
    import cbor_diag

    credentials = cbor2.loads(cbor_diag.diag2cbor(server_credentials_file.read_text()))

except ImportError:
    import json

    credentials = json.load(server_credentials_file.open("rb"))


async def main():
    global context

    # Code to start a task that regularly queries the gateway

    # task = asyncio.create_task(loop())
    # background_tasks.add(task)
    # task.add_done_callback(background_tasks.discard)

    server_credentials = CredentialsMap()

    root = Site()
    root.add_resource(["rd"], Register())

    server_credentials.load_from_dict(credentials)

    root = OscoreSiteWrapper(root, server_credentials)

    context = await Context.create_server_context(
        root,
        ("0.0.0.0", 4230),
        server_credentials=server_credentials,
        transports=list(["oscore", "udp6"]),  # udp6 and oscore for encrypted connection
    )

    context.client_credentials.load_from_dict(credentials)

    print("request interfaces", context.request_interfaces)
    print("CoAP server started")
    await asyncio.get_running_loop().create_future()


async def process_update(server: str):
    # We have to use the server's context so we use the same UDP port and can go through the NAT.
    # This uses the NAT the different routers have setup when the device did a request to this server. 
    global context
    if context is None:
        print("Error: uninitialized context")
        return
    print("sending request to server ", server)

    msg = Message(code=GET, uri=server + "/status")
    result = await context.request(msg).response

    print("received result: ", result)
    if result.code == Code.CONTENT:
        decoded = cbor2.loads(result.payload)
        print("cbor data:", decoded)
        body = convert_gateway_update(decoded)
        print("received result: ", json.dumps(body, indent=4))

        if BEARER_TOKEN is not None and BACKEND_ENDPOINT is not None:
            bearer = "Bearer " + BEARER_TOKEN
            headers = {"Authorization": bearer}
            async with aiohttp.ClientSession(headers=headers) as session:
                async with session.post(
                    BACKEND_ENDPOINT,
                    json=body,
                ) as response:
                    print("result: ", response.status, await response.text())
        else:
            print(
                "Cannot send request to backend: BEARER_TOKEN and BACKEND_ENDPOINT need to be set"
            )

    else:
        print("Got error code: ", result.code)


# This loop is not started. You can start it by uncommenting the few lines at the start of main()
# It can be used to regularly query the device for it's status.
async def loop():
    global context

    while True:
        await asyncio.sleep(70)

        print("Getting update from servers: ")
        for s in servers:
            process_update(s)


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    logging.getLogger("coap-server").setLevel(logging.INFO)

    asyncio.run(main())
