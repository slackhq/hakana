use type Facebook\XHP\HTML\img;
$url = (string)HH\global_get('_GET')['url'];
$image = <img src={$url} srcset={$url} />;
$copy = <img {...$image} />;
