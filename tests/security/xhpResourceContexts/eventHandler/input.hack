use type Facebook\XHP\HTML\img;
$url = (string)HH\global_get('_GET')['url'];
$element = <img onerror={$url} />;
