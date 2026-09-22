$parts = pathinfo((string)HH\global_get('_GET')['path']);
echo $parts['dirname'];
echo $parts['basename'];
echo $parts['filename'];
