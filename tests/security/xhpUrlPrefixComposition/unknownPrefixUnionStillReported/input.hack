use type Facebook\XHP\HTML\a;
function render(bool $relative, string $other, string $id): void {
    $query = (string)HH\global_get('_GET')['q'];
    $prefix = $relative ? '/apps/' : $other;
    $link = <a href={$prefix.$id.'?'.$query} />;
}
