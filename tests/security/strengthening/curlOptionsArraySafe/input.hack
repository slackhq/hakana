$ch = curl_init();
curl_setopt_array($ch, dict[CURLOPT_RETURNTRANSFER => (int)HH\global_get('_GET')['return']]);
