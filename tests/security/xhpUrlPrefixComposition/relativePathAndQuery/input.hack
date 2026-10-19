use type Facebook\XHP\HTML\a;
function render(string $id, string $section): void {
    $query = (string)HH\global_get('_GET')['q'];
    $link = <a href={'/apps/'.$id.'/'.$section.'?'.$query} />;
}
