use type Facebook\XHP\HTML\link;
$url = (string)HH\global_get('_GET')['url'];
$rel = (string)HH\global_get('_GET')['rel'];
$link = <link rel={$rel} href={$url} />;
