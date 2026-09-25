#!/bin/bash

########################################################################
# provisionDevice.sh
# 
# Provision our device by calling the Kuzzle APIs
#
# Henri SIMOES                                      - September, 2026
########################################################################

MAC_ADDRESS=$1
ASSET=$2
TENANT=tenant-geosecur-trackgoods
DEVICE_MODEL=Arielos
ASSET_MODEL=CaisseMobile

URL_BACKEND="https://geosecur-api.sta.innovation-laposte.io"
# TOKEN a lire soit  depuis l'environnement, ou a defaut ici interactivement

function usage () {
  echo "Usage: $0 <MAC address> <asset>" >&2
  exit 1
}

########################################################################
## Makes an HTTP request to the Kuzzle API
# If successfull, tries to read a specific field from the JSON result
# and prints it
# else displays the Kuzzle API error message if available
# Return 0 on sucess, 1 on any error
########################################################################
function httpRequest () {
    if [ $# -lt 4 ]; then
        echo "Usage : httpRequest VALUE_TO_READ JQ_QUERY METHOD URL [arguments httpie...]" >&2
        return 1
    fi
    local VALUE_TO_READ=$1
    local JQ_QUERY=$2
    local METHOD=$3
    local URL=$4
    shift 4
    local other_parameters=("$@")
    local response value code msg

    # echo "Nombre de paramètres : ${#other_parameters[@]}" >&2
    #echo "Paramètres : ${other_parameters[*]}" >&2
    if response=$(http --check-status --ignore-stdin --timeout=10 -A bearer -a "${TOKEN}" -b "${METHOD}" "${URL}" "${other_parameters[@]}"); then
        # HTTP request considered successfull
        value=$(jq -er "${JQ_QUERY}" <<< "$response") || {
            echo "${VALUE_TO_READ} could not be retrieved could not be retrieved" >&2
            return 1
        }
        echo "${value}"
        return 0
    else
        code=$?
        case $code in
            2) msg="Request timeout" ;;
            3) msg="Redirection not followed (HTTP 3xx), please add '--follow'" ;;
            4) msg="Client error (HTTP 4xx)" ;;
            5) msg="Server error (HTP 5xx)" ;;
            6) msg="Too much redirections" ;;
            *) msg="Network error or other (code $code)" ;;
        esac
        echo "$msg" >&2
        # [ -n "$response" ] && echo "Réponse : $response" >&2
        if [ -n "$response" ]; then
            # errorMsg=$(echo "$response" | jq -er '.error.message') && { echo "Kuzzle says : $errorMsg" >&2; } || echo "$response" >&2; 
            if errorMsg=$(jq -er '.error.message' <<< "$response"); then
                echo "Kuzzle says : $errorMsg" >&2; 
            else
                echo "$response" >&2; 
            fi
        fi
        return 1
    fi
}

if [ $# -ne 2 ]; then
	usage
fi

# install HTTPie first
command -v http >/dev/null 2>&1 || { echo "Erreur : l'utilitaire 'http' (HTTPie) est introuvable. Installez-le avec : sudo apt install httpie" >&2; exit 1; }

if [ -z ${TOKEN+x} ]; then
        echo "Enter token : "
        read -s TOKEN
fi


# https://geosecur-api.sta.innovation-laposte.io/_/device-manager/payload/${${DEVICE_MODEL}}

# Create device
id=$(httpRequest _id .result._id POST ${URL_BACKEND}/_/device-manager/${TENANT}/devices model=${DEVICE_MODEL} reference=${MAC_ADDRESS}) || {
        echo "Id could not be retrieved"
        exit 1;
    }
echo "Device created. Id : $id" 

# POST http://kuzzle:7512/_/device-manager/:engineId/assets/_search
# query='{bool:{filter:[{term:{model:"CaisseMobile"}},{term:{reference:"'${ASSET}'"}}]}}'
query=$(jq -nc --arg m "${ASSET_MODEL}" --arg r "$ASSET" \
    '{bool:{filter:[{term:{model:$m}},{term:{reference:$r}}]}}')
# echo $query
assetId=$(httpRequest _id '.result.hits[0]._id' POST ${URL_BACKEND}/_/device-manager/${TENANT}/assets/_search "query:=${query}") || {
        echo "assetId could not be retrieved"
        exit 1;
    }
echo "assetId=${assetId}"

# PUT http://kuzzle:7512/_/device-manager/:engineId/devices/:_id/_link/:assetId
status=$(httpRequest status .status PUT "${URL_BACKEND}/_/device-manager/${TENANT}/devices/${id}/_link/${assetId}" implicitMeasuresLinking==true) || {
        echo "Status could not be retrieved"
        exit 1;
    }
# if [ $status == 200 ; then]
echo "Device linked. Status : $status" 

exit 0

