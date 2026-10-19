use type Facebook\XHP\HTML\img;
use type Facebook\XHP\HTML\iframe;

$url = (string)HH\global_get('_GET')['url'];
$source = <img src={$url} />;
$spread = <iframe {...$source} />;
$safe = <iframe src={"/local"} />;
