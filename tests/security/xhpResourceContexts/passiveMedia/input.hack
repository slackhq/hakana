use type Facebook\XHP\HTML\{img, source, video, audio, track, input, body};
$url = (string)HH\global_get('_GET')['url'];
$image = <img src={$url} srcset={$url} alt={$url} />;
$source = <source src={$url} srcset={$url} />;
$video = <video src={$url} poster={$url} />;
$audio = <audio src={$url} />;
$track = <track src={$url} />;
$input = <input type="image" src={$url} />;
$background = <body background={$url} />;
