use type Facebook\XHP\HTML\form;

$input = (string)HH\global_get('_GET')['id'];
$source = <form action={"/submit"} id={$input} />;
$spread = <form {...$source} />;
