use type Facebook\XHP\HTML\iframe;
$url = (string)HH\global_get('_GET')['url'];
$element = <iframe srcdoc={$url} />;
