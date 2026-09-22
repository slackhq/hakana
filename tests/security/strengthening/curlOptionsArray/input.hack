$ch = curl_init();
curl_setopt_array($ch, dict[CURLOPT_URL => (string)HH\global_get('_GET')['url'], CURLOPT_RETURNTRANSFER => true]);
