use type Facebook\XHP\HTML\link;
$url = (string)HH\global_get('_GET')['url'];
$icon = <link rel="shortcut icon" href={$url} />;
$canonical = <link href={$url} rel="canonical" />;
$alternate = <link rel="alternate" type="application/rss+xml" href={$url} />;
$image = <link href={$url} rel="PRELOAD" as="IMAGE" type="image/png" imagesrcset={$url} />;
