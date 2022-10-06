class StringHolder {
    public $x;

    public function __construct(string $x) {
        $this->x = $x;
    }
}

$b = new StringHolder($_GET["x"]);

echo $b->x;