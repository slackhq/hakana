use type Facebook\XHP\HTML\{a, link};
$url = (string)HH\global_get('_GET')['url'];
$attributes = <a rel="stylesheet" />;
$link = <link rel="icon" {...$attributes} href={$url} />;
