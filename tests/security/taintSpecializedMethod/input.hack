<<\Hakana\SecurityAnalysis\SpecializeInstance()>>
class Unsafe {
    public function isUnsafe() {
        return $_GET["unsafe"];
    }
}
$a = new Unsafe();
echo $a->isUnsafe();