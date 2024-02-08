1) Install typescript
via npm
`npm install -g typescript`
more info at https://www.typescriptlang.org/download

2) Compile ts files
`tsc`

3) Run js file via nodejs
`node <file name>`


npm install -g @microsoft/api-extractor
npm install -g @microsoft/api-documenter


//then run to create `api-extractor.json` file
//api-extractor init


$ cd examples/nodejs/typescript

# First invoke the TypeScript compiler to make the .d.ts files
$ tsc

# Next, we invoke API Extractor
$ api-extractor run --local --verbose