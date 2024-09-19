<<aas()>>
final class StringHolder {
    public $x;

    public function __construct(string $x) {
        $this->x = $x;
    }
}

$a = new StringHolder("a");
$b = new StringHolder(HH\global_get('_GET')["x"]);

echo $a->x;