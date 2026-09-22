$files = HH\global_get('_FILES') as dict<_, _>;
echo (string)$files['upload']['tmp_name'];
