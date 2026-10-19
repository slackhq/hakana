use type Facebook\XHP\HTML\a;
function render(bool $absolute, string $id): void {
    $query = (string)HH\global_get('_GET')['q'];
    $prefix = $absolute ? 'https://example.test/apps/' : '/apps/';
    $link = <a href={$prefix.$id.'?'.$query} />;
}
