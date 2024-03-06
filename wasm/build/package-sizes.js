const fs = require('fs');
const path = require('path');

// Function to calculate human readable size
function humanReadableSize(size) {
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    while (size >= 1024 && i < units.length - 1) {
        size /= 1024;
        i++;
    }
    return `${size.toFixed(2)} ${units[i]}`;
}

// Function to calculate total size of files in a folder recursively
function calculateFolderSize(folderPath) {
    let totalSize = 0;
    const files = fs.readdirSync(folderPath);

    files.forEach(file => {
        const filePath = path.join(folderPath, file);
        const stats = fs.statSync(filePath);
        if (stats.isFile()) {
            totalSize += stats.size;
        } else if (stats.isDirectory()) {
            totalSize += calculateFolderSize(filePath);
        }
    });

    return totalSize;
}

// Function to scan folder, calculate total size, and generate JSON file
function generateFolderSizesJSON(folderPath, outputFileName) {
    const folders = fs.readdirSync(folderPath);
    const folderSizes = {};

    folders.forEach(folder => {
        const absoluteFolder = path.join(folderPath, folder);
        if (fs.statSync(absoluteFolder).isDirectory()) {
            const folderSize = calculateFolderSize(absoluteFolder);
            folderSizes[folder] = humanReadableSize(folderSize);
        }
    });

    const jsonContent = JSON.stringify(folderSizes, null, 2);
    const jsContent = "window.packageSizes = " + jsonContent + ";";
    fs.writeFileSync(outputFileName, jsContent);
    console.log(`Folder sizes JSON file generated successfully: ${outputFileName}`);
}

// Usage example
const folderPath = path.join(__dirname,'../web/');
const outputFileName = path.join(__dirname,'../package-sizes.js');

generateFolderSizesJSON(folderPath, outputFileName);
