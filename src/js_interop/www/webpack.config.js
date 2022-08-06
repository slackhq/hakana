const path = require('path');
const webpack = require('webpack');
const HTMLWebpackPlugin = require('html-webpack-plugin')

const CopyWebpackPlugin = require('copy-webpack-plugin')

module.exports = {
  mode: 'development',
  devtool: 'source-map',
  entry: './bootstrap.js',
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: 'bootstrap.js',
  },
  optimization: {
    chunkIds: "size"
  },
  target: 'web',
  module: {
    rules: [{
      test: /\.tsx?$/,
      exclude: /(node_modules)/,
      use: {
        loader: 'ts-loader',
        options: {
          happyPackMode: true,
        }
      },
    },
    {
      test: /\.css$/,
      use: ['style-loader', 'css-loader'],
    },
    {
      test: /onigasm\.wasm$/,
      use: {
        loader: 'file-loader',
        options: {
          name: '[name].[hash:6].[ext]'
        }
      },
      type: 'javascript/auto'
    }
    ],
  },
  resolve: {
    extensions: ['.js', '.ts', '.tsx'],
    fallback: {
      util: require.resolve("util/"),
      path: require.resolve("path-browserify")
    }
  },
  experiments: {
    syncWebAssembly: true
  },
  plugins: [
    new CopyWebpackPlugin(['index.html']),
    new HTMLWebpackPlugin({
      template: path.resolve('./index.html')
    }),
    new webpack.ProvidePlugin({
      // Make a global `process` variable that points to the `process` package,
      // because the `util` package expects there to be a global variable named `process`.
           // Thanks to https://stackoverflow.com/a/65018686/14239942
      process: 'process/browser'
   })
  ],
};



