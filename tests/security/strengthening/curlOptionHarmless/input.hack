$ch = curl_init();
curl_setopt($ch, CURLOPT_RETURNTRANSFER, (bool)HH\global_get('_GET')['q']);
