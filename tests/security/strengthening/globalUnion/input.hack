function read(bool $post): void {
$name = $post ? '_POST' : '_GET';
$data = HH\global_get($name) as dict<_, _>;
echo (string)$data['q'];
}
