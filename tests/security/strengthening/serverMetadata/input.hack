$server = HH\global_get('_SERVER') as dict<_, _>;
echo (string)$server['SERVER_SOFTWARE'];
