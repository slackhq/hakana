final class A2 {
    public static string $title = "foo";
    public static string $label = "bar";
}

$model = new A2();
$message = $model::$title;
$message .= $model::$label;
echo $message;