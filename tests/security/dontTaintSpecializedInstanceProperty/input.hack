<<\Hakana\SecurityAnalysis\SpecializeInstance()>>
class StringHolder {
    public $x;

    public function __construct(string $x) {
        $this->x = $x;
    }
}

$a = new StringHolder("a");
$b = new StringHolder($_GET["x"]);

echo $a->x;