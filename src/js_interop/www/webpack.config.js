const path = require('path')
const HTMLWebpackPlugin = require('html-webpack-plugin')

const CopyWebpackPlugin = require('copy-webpack-plugin')

module.exports = {
  mode: 'development',
  devtool: 'sourcemap',
  entry: './bootstrap.js',
  output: {
    path: path.resolve(__dirname, "dist"),
    filename: 'bootstrap.js',
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
    extensions: ['.js', '.ts', '.tsx']
  },
  plugins: [
    new CopyWebpackPlugin(['index.html']),
    new HTMLWebpackPlugin({
      template: path.resolve('./index.html')
    })
  ],
};



