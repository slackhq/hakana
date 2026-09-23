use type Facebook\XHP\HTML\a;
$path = (string)HH\global_get('_GET')['path'];
$link = <a href={'/'.$path.'?flag=1'} />;
