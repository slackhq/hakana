use type Facebook\XHP\HTML\link;
$url = (string)HH\global_get('_GET')['url'];
$link = <link href={$url} rel="preload" />;
