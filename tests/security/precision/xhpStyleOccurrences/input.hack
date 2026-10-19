use type Facebook\XHP\HTML\style;

$input = (string)HH\global_get('_GET')['css'];
$first = <style>{$input}</style>;
$second = <style>{$input}</style>;
$safe = <style>{"body { color: black; }"}</style>;
