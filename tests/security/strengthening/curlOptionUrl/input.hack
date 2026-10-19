<<Hakana\SecurityAnalysis\Source('RawUserData')>>
function input(): string { return ''; }
$ch = curl_init();
curl_setopt($ch, CURLOPT_URL, input());
