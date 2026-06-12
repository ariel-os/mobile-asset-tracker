## secrets 

First generate `server.diag` and `server.cosekey` in `coap-proxy` in the `coap-proxy` directory (parent directory).

```
sed -i 's/server.cosekey/\/etc\/coap-secret\/server.cosekey/' server.diag
kubectl create secret generic coap-secret --from-file=server.diag --from-file=server.cosekey
```


Create env file containing

```
BACKEND_ENDPOINT="https://some-endpoint.com" 
BEARER_TOKEN="dont-make-it-public"
```

Then push it 

```
kubectl create secret generic kuzzle-secret --from-env-file .env 
```
