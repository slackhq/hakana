use type Facebook\XHP\HTML\a;
function render(string $id): void {
    $query = (string)HH\global_get('_GET')['q'];
    $link = <a href={'https://example.test/apps/'.$id.'?'.$query} />;
}
