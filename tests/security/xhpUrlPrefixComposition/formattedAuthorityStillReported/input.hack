use type Facebook\XHP\HTML\a;
$host = (string)HH\global_get('_GET')['host'];
$link = <a href={HH\Lib\Str\format('https://%s/apps/%s', $host, 'fixed')} />;
