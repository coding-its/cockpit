<?php
/**
 * Installiert ein Plugin aus einer ZIP-Datei und aktualisiert die Update-Transients.
 * Aufruf: php wp-install-plugin.php <pfad-zur-zip>
 */

$_SERVER['HTTP_HOST'] = 'localhost:81';
$_SERVER['SERVER_NAME'] = 'localhost';
$_SERVER['SERVER_PORT'] = '81';
$_SERVER['REQUEST_URI'] = '/wordpress/';
$_SERVER['SCRIPT_NAME'] = '/wordpress/wp-admin/';
$_SERVER['PHP_SELF'] = '/wordpress/wp-admin/';
$_SERVER['HTTPS'] = '';
$_SERVER['REQUEST_METHOD'] = 'GET';

$zip = $argv[1] ?? '';
if (!is_file($zip)) {
    echo "ZIP nicht gefunden: $zip\n";
    exit(1);
}

require_once __DIR__ . '/wp-load.php';
require_once ABSPATH . 'wp-admin/includes/file.php';
require_once ABSPATH . 'wp-admin/includes/misc.php';
require_once ABSPATH . 'wp-admin/includes/plugin.php';
require_once ABSPATH . 'wp-admin/includes/update.php';
require_once ABSPATH . 'wp-admin/includes/class-wp-upgrader.php';

add_filter('filesystem_method', fn() => 'direct');

$skin = new Automatic_Upgrader_Skin();
$upgrader = new Plugin_Upgrader($skin);

ob_start();
$result = $upgrader->install($zip);
$output = trim(ob_get_clean());

if (is_wp_error($result)) {
    echo "FEHLER: " . $result->get_error_message() . "\n";
    exit(1);
}

echo "Installiert: $result\n";

wp_clean_plugins_cache(true);
wp_update_plugins();
delete_site_transient('update_plugins');
wp_update_plugins();

$updates = get_plugin_updates();
echo "Plugins mit Update: " . count($updates) . "\n";
foreach ($updates as $file => $data) {
    echo "  $file : {$data->Version} -> {$data->update->new_version}\n";
}
