use type Facebook\XHP\HTML\{img, script};
$url = (string)HH\global_get('_GET')['url'];
$image = <img src={$url} />;
$script = <script src={$url} />;
