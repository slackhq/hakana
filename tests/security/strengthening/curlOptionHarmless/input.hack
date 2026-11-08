$ch = curl_init();
curl_setopt($ch, CURLOPT_RETURNTRANSFER, (int)HH\global_get('_GET')['q']);
