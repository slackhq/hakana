function run(): void {
    $value = (string)HH\global_get('_POST')['value'];
    header('Content-Type: text/plain; charset=utf-8');
    echo $value;
}
