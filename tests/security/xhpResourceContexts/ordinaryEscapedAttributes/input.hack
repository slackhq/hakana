use type Facebook\XHP\HTML\{div, input};
$url = (string)HH\global_get('_GET')['url'];
$div = <div title={$url} data-value={$url} aria-label={$url} />;
$input = <input name={$url} value={$url} placeholder={$url} />;
