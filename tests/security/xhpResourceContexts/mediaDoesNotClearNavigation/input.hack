use type Facebook\XHP\HTML\{img, a};
$url = (string)HH\global_get('_GET')['url'];
$image = <img src={$url} />;
$link = <a href={$url} />;
