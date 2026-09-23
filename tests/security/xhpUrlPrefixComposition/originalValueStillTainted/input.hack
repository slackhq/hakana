use type Facebook\XHP\HTML\a;
function render(string $id): void {
    $query = (string)HH\global_get('_GET')['q'];
    $safe = <a href={'/apps/'.$id.'?'.$query} />;
    $unsafe = <a href={$query} />;
}
