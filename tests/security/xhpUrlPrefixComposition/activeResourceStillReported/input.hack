use type Facebook\XHP\HTML\script;
function render(string $id): void {
    $query = (string)HH\global_get('_GET')['q'];
    $script = <script src={'/assets/'.$id.'?'.$query} />;
}
