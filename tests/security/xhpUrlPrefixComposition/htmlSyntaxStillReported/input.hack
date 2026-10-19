function render(string $id): void {
    $query = (string)HH\global_get('_GET')['q'];
    echo '/apps/'.$id.'?'.$query;
}
