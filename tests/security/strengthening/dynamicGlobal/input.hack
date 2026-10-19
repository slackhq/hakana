function read_global(string $name): mixed { return HH\global_get($name); }
$data = read_global('_GET') as dict<_, _>;
echo (string)$data['q'];
