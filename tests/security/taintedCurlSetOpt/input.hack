$ch = curl_init();
curl_setopt($ch, CURLOPT_URL, HH\global_get('_GET')['url']);