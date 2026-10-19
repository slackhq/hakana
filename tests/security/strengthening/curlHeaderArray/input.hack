$ch = curl_init();
curl_setopt($ch, CURLOPT_HTTPHEADER, vec[(string)HH\global_get('_GET')['header']]);
