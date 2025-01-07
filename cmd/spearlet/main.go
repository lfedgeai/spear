package main

import (
	"strings"

	"github.com/lfedgeai/spear/pkg/common"
	spearlet "github.com/lfedgeai/spear/spearlet"
	"github.com/lfedgeai/spear/spearlet/task"
	log "github.com/sirupsen/logrus"
	"github.com/spf13/cobra"

	"os"
)

type SpearletConfig struct {
	Addr string
	Port string
}

var (
	execRtTypeStr    string
	execWorkloadName string
	execReqMethod    string
	execReqPayload   string

	runSpearAddr  string
	runSearchPath []string
	runVerbose    bool
	runDebug      bool

	serveAddr string
	servePort string
)

func NewRootCmd() *cobra.Command {
	var rootCmd = &cobra.Command{
		Use:   "spearlet",
		Short: "spearlet is the command line tool for the SPEAR spearlet",
		Run: func(cmd *cobra.Command, args []string) {
			cmd.Help()
		},
	}

	// exec subcommand
	var execCmd = &cobra.Command{
		Use:   "exec",
		Short: "Execute a workload",
		Run: func(cmd *cobra.Command, args []string) {
			var validChoices = map[string]task.TaskType{
				"docker":  task.TaskTypeDocker,
				"process": task.TaskTypeProcess,
				"dylib":   task.TaskTypeDylib,
				"wasm":    task.TaskTypeWasm,
			}

			if execWorkloadName == "" {
				log.Errorf("Invalid workload name %s", execWorkloadName)
				return
			}
			if execReqMethod == "" {
				log.Errorf("Invalid request method %s", execReqMethod)
				return
			}
			if runSpearAddr == "" {
				runSpearAddr = common.SpearPlatformAddress
			}

			// check if the workload type is valid
			if rtType, ok := validChoices[strings.ToLower(execRtTypeStr)]; !ok {
				log.Errorf("Invalid runtime type %s", execRtTypeStr)
			} else {
				log.Infof("Executing workload %s with runtime type %v", execWorkloadName, rtType)
				// set log level
				if runVerbose {
					spearlet.SetLogLevel(log.DebugLevel)
				}

				// create config
				config := spearlet.NewExecSpearletConfig(runDebug, runSpearAddr, runSearchPath)
				w := spearlet.NewSpearlet(config)
				w.Initialize()

				// lookup task id
				execWorkloadId, err := w.LookupTaskId(execWorkloadName)
				if err != nil {
					log.Errorf("Error looking up task id: %v", err)
					// print available tasks
					tasks := w.ListTasks()
					log.Infof("Available tasks: %v", tasks)
					w.Stop()
					return
				}

				res, err := w.ExecuteTask(execWorkloadId, rtType, true, execReqMethod, execReqPayload)
				if err != nil {
					log.Errorf("Error executing workload: %v", err)
				}
				log.Debugf("Workload execution result: %v", res)
				w.Stop()
				// TODO: implement workload execution
			}
		},
	}

	// workload id
	execCmd.PersistentFlags().StringVarP(&execWorkloadName, "name", "n", "", "workload name")
	// workload type, a choice of Docker, Process, Dylib or Wasm
	execCmd.PersistentFlags().StringVarP(&execRtTypeStr, "type", "t", "Docker", "type of the workload")
	// workload request payload
	execCmd.PersistentFlags().StringVarP(&execReqMethod, "method", "m", "handle", "default method to invoke")
	execCmd.PersistentFlags().StringVarP(&execReqPayload, "payload", "p", "", "request payload")
	rootCmd.AddCommand(execCmd)

	var serveCmd = &cobra.Command{
		Use:   "serve",
		Short: "Start the spearlet server",
		Run: func(cmd *cobra.Command, args []string) {
			// set log level
			if runVerbose {
				spearlet.SetLogLevel(log.DebugLevel)
			}

			if runSpearAddr == "" {
				runSpearAddr = common.SpearPlatformAddress
			}

			// create config
			config := spearlet.NewServeSpearletConfig(serveAddr, servePort, runSearchPath,
				runDebug, runSpearAddr)
			w := spearlet.NewSpearlet(config)
			w.Initialize()
			w.StartServer()
		},
	}
	// addr flag
	serveCmd.PersistentFlags().StringVarP(&serveAddr, "addr", "a", "localhost", "address of the server")
	// port flag
	serveCmd.PersistentFlags().StringVarP(&servePort, "port", "p", "8080", "port of the server")
	rootCmd.AddCommand(serveCmd)

	// spear platform address for workload to connect
	rootCmd.PersistentFlags().StringVarP(&runSpearAddr, "spear-addr", "s", os.Getenv("SPEAR_RPC_ADDR"), "SPEAR platform address for workload RPC")
	// search path
	rootCmd.PersistentFlags().StringArrayVarP(&runSearchPath, "search-path", "L", []string{}, "search path list for the spearlet")
	// verbose flag
	rootCmd.PersistentFlags().BoolVarP(&runVerbose, "verbose", "v", false, "verbose output")
	// debug flag
	rootCmd.PersistentFlags().BoolVarP(&runDebug, "debug", "d", false, "debug mode")
	return rootCmd
}

func main() {
	NewRootCmd().Execute()
}
