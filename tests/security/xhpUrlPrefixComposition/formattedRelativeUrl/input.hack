use type Facebook\XHP\HTML\a;
function render(string $id): void {
    $query = (string)HH\global_get('_GET')['q'];
    $link = <a href={HH\Lib\Str\format('/apps/%s?%s', $id, $query)} />;
}
