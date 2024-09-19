final class StringHolder {
    public string $x;

    public function __construct(string $x) {
        $this->x = $x;
    }
}

$b = new StringHolder(HH\global_get('_GET')["x"]);

echo $b->x;